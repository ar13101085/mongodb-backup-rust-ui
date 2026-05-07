use super::JsonFile;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub label: String,
    pub uri: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConnectionsFile {
    pub connections: Vec<Connection>,
}

pub struct ConnectionStore(pub JsonFile<ConnectionsFile>);

impl ConnectionStore {
    pub async fn load(data_dir: &std::path::Path) -> AppResult<Self> {
        Ok(Self(
            JsonFile::load_or_init(PathBuf::from(data_dir).join("connections.json")).await?,
        ))
    }

    pub async fn list(&self) -> Vec<Connection> {
        self.0.read().await.connections.clone()
    }

    pub async fn get(&self, id: Uuid) -> Option<Connection> {
        self.0
            .read()
            .await
            .connections
            .iter()
            .find(|c| c.id == id)
            .cloned()
    }

    pub async fn create(&self, label: String, uri: String) -> AppResult<Connection> {
        self.0
            .try_update(|state| {
                if state.connections.iter().any(|c| c.label.eq_ignore_ascii_case(&label)) {
                    return Err(AppError::Conflict("connection label already exists".into()));
                }
                let conn = Connection {
                    id: Uuid::new_v4(),
                    label,
                    uri,
                    created_at: Utc::now(),
                };
                state.connections.push(conn.clone());
                Ok(conn)
            })
            .await
    }

    pub async fn update(&self, id: Uuid, label: String, uri: String) -> AppResult<Connection> {
        self.0
            .try_update(|state| {
                if state
                    .connections
                    .iter()
                    .any(|c| c.id != id && c.label.eq_ignore_ascii_case(&label))
                {
                    return Err(AppError::Conflict("connection label already exists".into()));
                }
                let conn = state
                    .connections
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or(AppError::NotFound)?;
                conn.label = label;
                conn.uri = uri;
                Ok(conn.clone())
            })
            .await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.0
            .try_update(|state| {
                let before = state.connections.len();
                state.connections.retain(|c| c.id != id);
                if state.connections.len() == before {
                    return Err(AppError::NotFound);
                }
                Ok(())
            })
            .await
    }
}
