use crate::auth::extractor::{AuthUser, SESSION_USER_KEY};
use crate::auth::password::{hash_password, verify_password};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use actix_session::Session;
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
pub struct CredentialsInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: uuid::Uuid,
    pub username: String,
    pub is_admin: bool,
}

impl From<&crate::storage::users::User> for UserView {
    fn from(u: &crate::storage::users::User) -> Self {
        Self {
            id: u.id,
            username: u.username.clone(),
            is_admin: u.is_admin,
        }
    }
}

/// GET /api/auth/status — used by the frontend to decide which page to render.
pub async fn status(state: web::Data<AppState>, session: Session) -> AppResult<HttpResponse> {
    let needs_setup = state.users.is_empty().await;
    let user_id = session
        .get::<uuid::Uuid>(SESSION_USER_KEY)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let user = match user_id {
        Some(id) => state.users.find_by_id(id).await,
        None => None,
    };
    Ok(HttpResponse::Ok().json(json!({
        "needs_setup": needs_setup,
        "user": user.as_ref().map(UserView::from),
    })))
}

/// POST /api/auth/setup — creates the first admin. Only valid while users.json is empty.
pub async fn setup(
    state: web::Data<AppState>,
    session: Session,
    body: web::Json<CredentialsInput>,
) -> AppResult<HttpResponse> {
    if !state.users.is_empty().await {
        return Err(AppError::Conflict("setup is closed; admin already exists".into()));
    }
    validate_credentials(&body)?;
    let hash = hash_password(&body.password)?;
    let user = state
        .users
        .create(body.username.trim().to_string(), hash, true)
        .await?;
    session
        .insert(SESSION_USER_KEY, user.id)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(UserView::from(&user)))
}

pub async fn login(
    state: web::Data<AppState>,
    session: Session,
    body: web::Json<CredentialsInput>,
) -> AppResult<HttpResponse> {
    if state.users.is_empty().await {
        return Err(AppError::BadRequest(
            "no users exist; complete setup first".into(),
        ));
    }
    let user = state
        .users
        .find_by_username(&body.username)
        .await
        .ok_or(AppError::Unauthorized)?;
    if !verify_password(&body.password, &user.password_hash) {
        return Err(AppError::Unauthorized);
    }
    session
        .insert(SESSION_USER_KEY, user.id)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(HttpResponse::Ok().json(UserView::from(&user)))
}

pub async fn logout(session: Session) -> AppResult<HttpResponse> {
    session.purge();
    Ok(HttpResponse::Ok().json(json!({ "ok": true })))
}

pub async fn me(user: AuthUser) -> AppResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(UserView::from(&user.0)))
}

fn validate_credentials(body: &CredentialsInput) -> AppResult<()> {
    let u = body.username.trim();
    if u.is_empty() || u.len() > 64 {
        return Err(AppError::BadRequest(
            "username must be 1-64 characters".into(),
        ));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}
