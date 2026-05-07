use crate::error::AppError;
use crate::state::AppState;
use crate::storage::users::User;
use actix_session::SessionExt;
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use std::future::Future;
use std::pin::Pin;
use uuid::Uuid;

pub const SESSION_USER_KEY: &str = "user_id";

pub struct AuthUser(pub User);

impl FromRequest for AuthUser {
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let session = req.get_session();
        let state = req
            .app_data::<actix_web::web::Data<AppState>>()
            .cloned();
        Box::pin(async move {
            let state = state.ok_or_else(|| AppError::Internal("missing AppState".into()))?;
            let user_id: Option<Uuid> = session
                .get::<Uuid>(SESSION_USER_KEY)
                .map_err(|e| AppError::Internal(format!("session read: {e}")))?;
            let user_id = user_id.ok_or(AppError::Unauthorized)?;
            let user = state
                .users
                .find_by_id(user_id)
                .await
                .ok_or(AppError::Unauthorized)?;
            Ok(AuthUser(user))
        })
    }
}
