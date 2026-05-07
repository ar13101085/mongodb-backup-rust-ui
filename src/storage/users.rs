use super::JsonFile;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UsersFile {
    pub users: Vec<User>,
}

pub struct UserStore(pub JsonFile<UsersFile>);

impl UserStore {
    pub async fn load(data_dir: &std::path::Path) -> AppResult<Self> {
        Ok(Self(
            JsonFile::load_or_init(PathBuf::from(data_dir).join("users.json")).await?,
        ))
    }

    pub async fn is_empty(&self) -> bool {
        self.0.read().await.users.is_empty()
    }

    pub async fn find_by_username(&self, username: &str) -> Option<User> {
        self.0
            .read()
            .await
            .users
            .iter()
            .find(|u| u.username.eq_ignore_ascii_case(username))
            .cloned()
    }

    pub async fn find_by_id(&self, id: Uuid) -> Option<User> {
        self.0.read().await.users.iter().find(|u| u.id == id).cloned()
    }

    pub async fn create(&self, username: String, password_hash: String, is_admin: bool) -> AppResult<User> {
        self.0
            .try_update(|state| {
                if state
                    .users
                    .iter()
                    .any(|u| u.username.eq_ignore_ascii_case(&username))
                {
                    return Err(AppError::Conflict("username already exists".into()));
                }
                let user = User {
                    id: Uuid::new_v4(),
                    username,
                    password_hash,
                    is_admin,
                    created_at: Utc::now(),
                };
                state.users.push(user.clone());
                Ok(user)
            })
            .await
    }
}
