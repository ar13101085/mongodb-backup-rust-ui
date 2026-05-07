use crate::backup::{BackupRunner, JobRegistry};
use crate::config::Config;
use crate::error::AppResult;
use crate::storage::connections::ConnectionStore;
use crate::storage::schedules::ScheduleStore;
use crate::storage::users::UserStore;

pub struct AppState {
    pub users: UserStore,
    pub connections: ConnectionStore,
    pub schedules: ScheduleStore,
    pub runner: BackupRunner,
    pub jobs: JobRegistry,
}

impl AppState {
    pub async fn init(config: &Config) -> AppResult<Self> {
        tokio::fs::create_dir_all(&config.data_dir).await?;
        tokio::fs::create_dir_all(&config.backup_dir).await?;

        let users = UserStore::load(&config.data_dir).await?;
        let connections = ConnectionStore::load(&config.data_dir).await?;
        let schedules = ScheduleStore::load(&config.data_dir).await?;
        let runner = BackupRunner::new(config.backup_dir.clone());
        let jobs = JobRegistry::new();
        Ok(Self {
            users,
            connections,
            schedules,
            runner,
            jobs,
        })
    }
}
