use crate::auth::AuthUser;
use crate::backup::{JobKind, JobSource, ProgressSink};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ServerFileRestore {
    pub filename: String,
    pub target_connection_id: Option<Uuid>,
    pub target_uri: Option<String>,
    pub target_database: Option<String>,
    #[serde(default)]
    pub drop_existing: bool,
}

/// POST /api/restore/server — restore an archive that already lives on the server.
pub async fn from_server(
    _user: AuthUser,
    state: web::Data<AppState>,
    body: web::Json<ServerFileRestore>,
) -> AppResult<HttpResponse> {
    let archive_path = state.runner.resolve(&body.filename)?;
    if !fs::try_exists(&archive_path).await? {
        return Err(AppError::NotFound);
    }
    let uri = resolve_target_uri(&state, body.target_connection_id, body.target_uri.as_deref()).await?;

    let (target_label, target_db_label) = describe_target(
        &state,
        body.target_connection_id,
        body.target_uri.as_deref(),
        body.target_database.as_deref(),
        &archive_path,
    )
    .await;
    let job_id = state
        .jobs
        .start(
            JobKind::Restore,
            body.target_connection_id,
            target_label,
            target_db_label,
            JobSource::Manual,
        )
        .await;

    let outcome = state
        .runner
        .restore(
            &uri,
            &archive_path,
            body.target_database.as_deref(),
            body.drop_existing,
            Some(ProgressSink {
                jobs: &state.jobs,
                job_id,
            }),
        )
        .await;

    let job_result = match &outcome {
        Ok(()) => Ok(format!("restored from {}", body.filename)),
        Err(e) => Err(e.to_string()),
    };
    state.jobs.finish(job_id, job_result).await;

    outcome?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

/// POST /api/restore/upload — multipart upload + restore.
/// Form fields:
///   - file: the .archive.gz file (required)
///   - target_connection_id: uuid (optional)
///   - target_uri: string (optional; required if no target_connection_id)
///   - target_database: string (optional)
///   - drop_existing: "true" / "false" (optional, defaults false)
pub async fn from_upload(
    _user: AuthUser,
    state: web::Data<AppState>,
    mut payload: Multipart,
) -> AppResult<HttpResponse> {
    let mut tmp_path: Option<PathBuf> = None;
    let mut target_connection_id: Option<Uuid> = None;
    let mut target_uri: Option<String> = None;
    let mut target_database: Option<String> = None;
    let mut drop_existing = false;

    let upload_dir = state.runner.backup_dir().join("_uploads");
    fs::create_dir_all(&upload_dir).await?;

    while let Some(mut field) = payload.try_next().await.map_err(|e| AppError::Multipart(e.to_string()))? {
        let name = field
            .content_disposition()
            .and_then(|cd| cd.get_name().map(str::to_string))
            .unwrap_or_default();

        match name.as_str() {
            "file" => {
                let unique = Uuid::new_v4();
                let path = upload_dir.join(format!("upload-{unique}.archive.gz"));
                let mut file = fs::File::create(&path).await?;
                while let Some(chunk) = field
                    .try_next()
                    .await
                    .map_err(|e| AppError::Multipart(e.to_string()))?
                {
                    file.write_all(&chunk).await?;
                }
                file.flush().await?;
                tmp_path = Some(path);
            }
            "target_connection_id" => {
                let value = read_text(&mut field).await?;
                if !value.is_empty() {
                    target_connection_id = Some(
                        Uuid::parse_str(&value)
                            .map_err(|_| AppError::BadRequest("invalid target_connection_id".into()))?,
                    );
                }
            }
            "target_uri" => {
                let value = read_text(&mut field).await?;
                if !value.is_empty() {
                    target_uri = Some(value);
                }
            }
            "target_database" => {
                let value = read_text(&mut field).await?;
                if !value.is_empty() {
                    target_database = Some(value);
                }
            }
            "drop_existing" => {
                let value = read_text(&mut field).await?;
                drop_existing = matches!(value.as_str(), "1" | "true" | "on" | "yes");
            }
            _ => {
                // Drain unknown fields.
                while field
                    .try_next()
                    .await
                    .map_err(|e| AppError::Multipart(e.to_string()))?
                    .is_some()
                {}
            }
        }
    }

    let archive_path = tmp_path.ok_or_else(|| AppError::BadRequest("missing file field".into()))?;
    let uri = match resolve_target_uri(&state, target_connection_id, target_uri.as_deref()).await {
        Ok(u) => u,
        Err(e) => {
            let _ = fs::remove_file(&archive_path).await;
            return Err(e);
        }
    };

    let (target_label, target_db_label) = describe_target(
        &state,
        target_connection_id,
        target_uri.as_deref(),
        target_database.as_deref(),
        &archive_path,
    )
    .await;
    let job_id = state
        .jobs
        .start(
            JobKind::Restore,
            target_connection_id,
            target_label,
            target_db_label,
            JobSource::Manual,
        )
        .await;

    let result = state
        .runner
        .restore(
            &uri,
            &archive_path,
            target_database.as_deref(),
            drop_existing,
            Some(ProgressSink {
                jobs: &state.jobs,
                job_id,
            }),
        )
        .await;

    let job_result = match &result {
        Ok(()) => Ok("restored from upload".to_string()),
        Err(e) => Err(e.to_string()),
    };
    state.jobs.finish(job_id, job_result).await;

    let _ = fs::remove_file(&archive_path).await;
    result?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

/// Build a friendly (connection_label, database_label) pair to describe the restore target.
/// Falls back to host from URI / database name parsed from the archive filename.
async fn describe_target(
    state: &AppState,
    connection_id: Option<Uuid>,
    explicit_uri: Option<&str>,
    target_database: Option<&str>,
    archive_path: &Path,
) -> (String, String) {
    let conn_label = if let Some(id) = connection_id {
        state
            .connections
            .get(id)
            .await
            .map(|c| c.label)
            .unwrap_or_else(|| "(deleted connection)".to_string())
    } else if let Some(uri) = explicit_uri {
        host_from_uri(uri).unwrap_or_else(|| "(custom uri)".to_string())
    } else {
        "(unknown target)".to_string()
    };

    let db_label = match target_database {
        Some(db) if !db.is_empty() => db.to_string(),
        _ => archive_source_db(archive_path).unwrap_or_else(|| "(as in archive)".to_string()),
    };

    (conn_label, db_label)
}

fn host_from_uri(uri: &str) -> Option<String> {
    let after_scheme = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let after_auth = after_scheme.split_once('@').map(|(_, r)| r).unwrap_or(after_scheme);
    let host = after_auth
        .split(|c| c == '/' || c == '?')
        .next()
        .unwrap_or("");
    if host.is_empty() { None } else { Some(host.to_string()) }
}

/// Backups are named `{conn_uuid}__{db}__{timestamp}.archive.gz`. Extract the db.
fn archive_source_db(archive_path: &Path) -> Option<String> {
    let name = archive_path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".archive.gz").unwrap_or(name);
    let parts: Vec<&str> = stem.split("__").collect();
    if parts.len() == 3 { Some(parts[1].to_string()) } else { None }
}

async fn read_text(field: &mut actix_multipart::Field) -> AppResult<String> {
    let mut buf = Vec::new();
    while let Some(chunk) = field
        .try_next()
        .await
        .map_err(|e| AppError::Multipart(e.to_string()))?
    {
        buf.extend_from_slice(&chunk);
        if buf.len() > 4096 {
            return Err(AppError::BadRequest("text field too large".into()));
        }
    }
    Ok(String::from_utf8_lossy(&buf).trim().to_string())
}

async fn resolve_target_uri(
    state: &AppState,
    connection_id: Option<Uuid>,
    explicit_uri: Option<&str>,
) -> AppResult<String> {
    if let Some(id) = connection_id {
        let conn = state
            .connections
            .get(id)
            .await
            .ok_or_else(|| AppError::BadRequest("unknown target_connection_id".into()))?;
        return Ok(conn.uri);
    }
    if let Some(uri) = explicit_uri {
        let trimmed = uri.trim();
        if !(trimmed.starts_with("mongodb://") || trimmed.starts_with("mongodb+srv://")) {
            return Err(AppError::BadRequest(
                "target_uri must start with mongodb:// or mongodb+srv://".into(),
            ));
        }
        return Ok(trimmed.to_string());
    }
    Err(AppError::BadRequest(
        "must provide target_connection_id or target_uri".into(),
    ))
}
