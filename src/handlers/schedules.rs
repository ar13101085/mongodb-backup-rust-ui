use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::schedules::{Schedule, ScheduleInput};
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ScheduleBody {
    pub connection_id: Uuid,
    pub database: String,
    pub interval_minutes: u32,
    pub retention: u32,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct ScheduleView {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub database: String,
    pub interval_minutes: u32,
    pub retention: u32,
    pub enabled: bool,
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_status: Option<String>,
}

impl From<&Schedule> for ScheduleView {
    fn from(s: &Schedule) -> Self {
        Self {
            id: s.id,
            connection_id: s.connection_id,
            database: s.database.clone(),
            interval_minutes: s.interval_minutes,
            retention: s.retention,
            enabled: s.enabled,
            last_run_at: s.last_run_at,
            last_status: s.last_status.clone(),
        }
    }
}

pub async fn list(_user: AuthUser, state: web::Data<AppState>) -> AppResult<HttpResponse> {
    let schedules = state.schedules.list().await;
    let view: Vec<_> = schedules.iter().map(ScheduleView::from).collect();
    Ok(HttpResponse::Ok().json(view))
}

pub async fn upsert(
    _user: AuthUser,
    state: web::Data<AppState>,
    body: web::Json<ScheduleBody>,
) -> AppResult<HttpResponse> {
    if state.connections.get(body.connection_id).await.is_none() {
        return Err(AppError::BadRequest("unknown connection_id".into()));
    }
    let s = state
        .schedules
        .upsert(ScheduleInput {
            connection_id: body.connection_id,
            database: body.database.trim().to_string(),
            interval_minutes: body.interval_minutes,
            retention: body.retention,
            enabled: body.enabled,
        })
        .await?;
    Ok(HttpResponse::Ok().json(ScheduleView::from(&s)))
}

pub async fn delete(
    _user: AuthUser,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    state.schedules.delete(path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}
