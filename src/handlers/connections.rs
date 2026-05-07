use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::connections::Connection;
use actix_web::{web, HttpResponse};
use mongodb::options::ClientOptions;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ConnectionInput {
    pub label: String,
    pub uri: String,
}

#[derive(Deserialize)]
pub struct ConnectionUpdate {
    pub label: String,
    /// Optional. If omitted/empty, the existing URI is kept.
    #[serde(default)]
    pub uri: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectionView {
    pub id: Uuid,
    pub label: String,
    pub uri_masked: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&Connection> for ConnectionView {
    fn from(c: &Connection) -> Self {
        Self {
            id: c.id,
            label: c.label.clone(),
            uri_masked: mask_uri(&c.uri),
            created_at: c.created_at,
        }
    }
}

pub async fn list(_user: AuthUser, state: web::Data<AppState>) -> AppResult<HttpResponse> {
    let connections = state.connections.list().await;
    let view: Vec<_> = connections.iter().map(ConnectionView::from).collect();
    Ok(HttpResponse::Ok().json(view))
}

pub async fn create(
    _user: AuthUser,
    state: web::Data<AppState>,
    body: web::Json<ConnectionInput>,
) -> AppResult<HttpResponse> {
    validate(&body)?;
    let conn = state
        .connections
        .create(body.label.trim().to_string(), body.uri.trim().to_string())
        .await?;
    Ok(HttpResponse::Ok().json(ConnectionView::from(&conn)))
}

pub async fn update(
    _user: AuthUser,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    body: web::Json<ConnectionUpdate>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    if body.label.trim().is_empty() {
        return Err(AppError::BadRequest("label is required".into()));
    }
    let new_uri = match body.uri.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(uri) => {
            if !(uri.starts_with("mongodb://") || uri.starts_with("mongodb+srv://")) {
                return Err(AppError::BadRequest(
                    "uri must start with mongodb:// or mongodb+srv://".into(),
                ));
            }
            uri.to_string()
        }
        None => {
            let existing = state.connections.get(id).await.ok_or(AppError::NotFound)?;
            existing.uri
        }
    };
    let conn = state
        .connections
        .update(id, body.label.trim().to_string(), new_uri)
        .await?;
    Ok(HttpResponse::Ok().json(ConnectionView::from(&conn)))
}

pub async fn delete(
    _user: AuthUser,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let id = path.into_inner();
    state.connections.delete(id).await?;
    state.schedules.delete_for_connection(id).await?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

/// GET /api/connections/:id/databases — connects to MongoDB and lists databases.
pub async fn list_databases(
    _user: AuthUser,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let conn = state
        .connections
        .get(path.into_inner())
        .await
        .ok_or(AppError::NotFound)?;
    let names = list_db_names(&conn.uri).await?;
    Ok(HttpResponse::Ok().json(names))
}

/// POST /api/connections/test — body: { uri }. Returns database list without saving.
pub async fn test(
    _user: AuthUser,
    body: web::Json<TestInput>,
) -> AppResult<HttpResponse> {
    let names = list_db_names(&body.uri).await?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true, "databases": names })))
}

#[derive(Deserialize)]
pub struct TestInput {
    pub uri: String,
}

async fn list_db_names(uri: &str) -> AppResult<Vec<String>> {
    let mut opts = ClientOptions::parse(uri)
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid connection string: {e}")))?;
    opts.server_selection_timeout = Some(Duration::from_secs(5));
    opts.connect_timeout = Some(Duration::from_secs(5));
    opts.app_name = Some("mongodb-utils".to_string());
    let client = Client::with_options(opts)?;
    let mut names = client.list_database_names().await?;
    names.retain(|n| !matches!(n.as_str(), "admin" | "local" | "config"));
    names.sort();
    Ok(names)
}

fn validate(input: &ConnectionInput) -> AppResult<()> {
    if input.label.trim().is_empty() {
        return Err(AppError::BadRequest("label is required".into()));
    }
    let uri = input.uri.trim();
    if !(uri.starts_with("mongodb://") || uri.starts_with("mongodb+srv://")) {
        return Err(AppError::BadRequest(
            "uri must start with mongodb:// or mongodb+srv://".into(),
        ));
    }
    Ok(())
}

/// Replace user:password in mongodb URIs with user:***
fn mask_uri(uri: &str) -> String {
    let Some(scheme_end) = uri.find("://") else {
        return uri.to_string();
    };
    let after_scheme = &uri[scheme_end + 3..];
    let Some(at_pos) = after_scheme.find('@') else {
        return uri.to_string();
    };
    let creds = &after_scheme[..at_pos];
    let rest = &after_scheme[at_pos..];
    let user = creds.split(':').next().unwrap_or("");
    let masked_creds = if creds.contains(':') {
        format!("{user}:***")
    } else {
        user.to_string()
    };
    format!("{}://{}{}", &uri[..scheme_end], masked_creds, rest)
}
