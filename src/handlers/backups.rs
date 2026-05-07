use crate::auth::AuthUser;
use crate::backup::{JobKind, JobSource, ProgressSink};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

pub async fn list(_user: AuthUser, state: web::Data<AppState>) -> AppResult<HttpResponse> {
    let files = state.runner.list().await?;
    Ok(HttpResponse::Ok().json(files))
}

/// GET /api/backups/jobs — snapshot of running + recent jobs.
pub async fn jobs(_user: AuthUser, state: web::Data<AppState>) -> AppResult<HttpResponse> {
    let jobs = state.jobs.list().await;
    Ok(HttpResponse::Ok().json(jobs))
}

/// GET /api/backups/jobs/stream — Server-Sent Events stream of job state changes.
/// Sends an initial `snapshot` event with current jobs, then `started`/`finished` as they happen.
pub async fn jobs_stream(_user: AuthUser, state: web::Data<AppState>) -> HttpResponse {
    use futures_util::stream::StreamExt;
    use std::time::Duration;
    use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

    let snapshot = crate::backup::jobs::JobEvent::Snapshot {
        jobs: state.jobs.list().await,
    };
    let rx = state.jobs.subscribe();

    let initial = futures_util::stream::once(async move { sse_format(&snapshot) });

    let live = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(ev) => Some(sse_format(&ev)),
            Err(_) => None, // lagged subscriber — the next event will resync
        }
    });

    let heartbeat = IntervalStream::new(tokio::time::interval(Duration::from_secs(15)))
        .map(|_| ":hb\n\n".to_string());

    let merged = futures_util::stream::select(live, heartbeat);
    let body = initial.chain(merged).map(|s: String| {
        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(s))
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(body)
}

fn sse_format<T: serde::Serialize>(value: &T) -> String {
    let payload = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    format!("data: {payload}\n\n")
}

#[derive(Deserialize)]
pub struct RunInput {
    pub connection_id: Uuid,
    pub database: String,
    /// Optional: if provided, link this run to a schedule (records last_run/status, then prunes).
    pub schedule_id: Option<Uuid>,
}

/// POST /api/backups/run — manually trigger a backup. Synchronous; returns when mongodump finishes.
pub async fn run_now(
    _user: AuthUser,
    state: web::Data<AppState>,
    body: web::Json<RunInput>,
) -> AppResult<HttpResponse> {
    let conn = state
        .connections
        .get(body.connection_id)
        .await
        .ok_or(AppError::NotFound)?;
    let started = Utc::now();
    let job_id = state
        .jobs
        .start(
            JobKind::Backup,
            Some(conn.id),
            conn.label.clone(),
            body.database.clone(),
            JobSource::Manual,
        )
        .await;

    let outcome = state
        .runner
        .run(
            &conn.uri,
            &body.database,
            conn.id,
            Some(ProgressSink {
                jobs: &state.jobs,
                job_id,
            }),
        )
        .await;

    let job_result = match &outcome {
        Ok(p) => Ok(p.file_name().and_then(|s| s.to_str()).unwrap_or("ok").to_string()),
        Err(e) => Err(e.to_string()),
    };
    state.jobs.finish(job_id, job_result).await;

    if let Some(schedule_id) = body.schedule_id {
        let status = match &outcome {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        };
        let _ = state.schedules.record_run(schedule_id, started, status).await;
    }

    let path = outcome?;

    if let Some(sched) = body.schedule_id {
        if let Some(s) = state.schedules.get(sched).await {
            let _ = state.runner.prune(conn.id, &body.database, s.retention).await;
        }
    }

    Ok(HttpResponse::Ok().json(json!({
        "ok": true,
        "filename": path.file_name().and_then(|s| s.to_str()).unwrap_or_default(),
    })))
}

pub async fn delete(
    _user: AuthUser,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let filename = path.into_inner();
    state.runner.resolve(&filename)?; // validates name
    state.runner.delete(&filename).await?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

pub async fn download(
    _user: AuthUser,
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> AppResult<HttpResponse> {
    let filename = path.into_inner();
    let resolved = state.runner.resolve(&filename)?;
    let file = NamedFile::open_async(&resolved)
        .await
        .map_err(|_| AppError::NotFound)?;
    Ok(file
        .use_last_modified(true)
        .set_content_disposition(actix_web::http::header::ContentDisposition {
            disposition: actix_web::http::header::DispositionType::Attachment,
            parameters: vec![actix_web::http::header::DispositionParam::Filename(
                filename,
            )],
        })
        .into_response(&req))
}
