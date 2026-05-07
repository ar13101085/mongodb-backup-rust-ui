use super::JsonFile;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub database: String,
    pub interval_minutes: u32,
    pub retention: u32,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SchedulesFile {
    pub schedules: Vec<Schedule>,
}

pub struct ScheduleStore(pub JsonFile<SchedulesFile>);

#[derive(Debug, Clone)]
pub struct ScheduleInput {
    pub connection_id: Uuid,
    pub database: String,
    pub interval_minutes: u32,
    pub retention: u32,
    pub enabled: bool,
}

impl ScheduleStore {
    pub async fn load(data_dir: &std::path::Path) -> AppResult<Self> {
        Ok(Self(
            JsonFile::load_or_init(PathBuf::from(data_dir).join("schedules.json")).await?,
        ))
    }

    pub async fn list(&self) -> Vec<Schedule> {
        self.0.read().await.schedules.clone()
    }

    pub async fn get(&self, id: Uuid) -> Option<Schedule> {
        self.0
            .read()
            .await
            .schedules
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub async fn upsert(&self, input: ScheduleInput) -> AppResult<Schedule> {
        validate(&input)?;
        self.0
            .try_update(|state| {
                if let Some(existing) = state
                    .schedules
                    .iter_mut()
                    .find(|s| s.connection_id == input.connection_id && s.database == input.database)
                {
                    existing.interval_minutes = input.interval_minutes;
                    existing.retention = input.retention;
                    existing.enabled = input.enabled;
                    return Ok(existing.clone());
                }
                let s = Schedule {
                    id: Uuid::new_v4(),
                    connection_id: input.connection_id,
                    database: input.database,
                    interval_minutes: input.interval_minutes,
                    retention: input.retention,
                    enabled: input.enabled,
                    last_run_at: None,
                    last_status: None,
                    created_at: Utc::now(),
                };
                state.schedules.push(s.clone());
                Ok(s)
            })
            .await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.0
            .try_update(|state| {
                let before = state.schedules.len();
                state.schedules.retain(|s| s.id != id);
                if state.schedules.len() == before {
                    return Err(AppError::NotFound);
                }
                Ok(())
            })
            .await
    }

    pub async fn delete_for_connection(&self, connection_id: Uuid) -> AppResult<()> {
        self.0
            .update(|state| state.schedules.retain(|s| s.connection_id != connection_id))
            .await
    }

    pub async fn record_run(
        &self,
        id: Uuid,
        when: DateTime<Utc>,
        status: String,
    ) -> AppResult<()> {
        self.0
            .try_update(|state| {
                let s = state
                    .schedules
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or(AppError::NotFound)?;
                s.last_run_at = Some(when);
                s.last_status = Some(status);
                Ok(())
            })
            .await
    }
}

fn validate(input: &ScheduleInput) -> AppResult<()> {
    if input.database.trim().is_empty() {
        return Err(AppError::BadRequest("database is required".into()));
    }
    if input.interval_minutes == 0 {
        return Err(AppError::BadRequest("interval_minutes must be > 0".into()));
    }
    if input.retention == 0 {
        return Err(AppError::BadRequest("retention must be > 0".into()));
    }
    Ok(())
}
