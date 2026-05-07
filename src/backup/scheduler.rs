use crate::backup::{JobKind, JobSource, ProgressSink};
use crate::state::AppState;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

const TICK_SECONDS: u64 = 30;

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(TICK_SECONDS));
        // First tick fires immediately; skip it so we don't slam everything at startup.
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = run_due(&state).await {
                tracing::error!(error = %e, "scheduler iteration failed");
            }
        }
    });
}

async fn run_due(state: &Arc<AppState>) -> crate::error::AppResult<()> {
    let now = Utc::now();
    let schedules = state.schedules.list().await;
    for s in schedules {
        if !s.enabled {
            continue;
        }
        let due = match s.last_run_at {
            Some(t) => now - t >= chrono::Duration::minutes(s.interval_minutes as i64),
            None => true,
        };
        if !due {
            continue;
        }
        let Some(conn) = state.connections.get(s.connection_id).await else {
            tracing::warn!(schedule = %s.id, "skipping schedule: connection missing");
            continue;
        };
        let state_cloned = state.clone();
        let schedule_id = s.id;
        let database = s.database.clone();
        let retention = s.retention;
        let conn_id = conn.id;
        let conn_label = conn.label.clone();
        let uri = conn.uri.clone();

        tokio::spawn(async move {
            let started = Utc::now();
            let job_id = state_cloned
                .jobs
                .start(
                    JobKind::Backup,
                    Some(conn_id),
                    conn_label,
                    database.clone(),
                    JobSource::Scheduled,
                )
                .await;
            let outcome = state_cloned
                .runner
                .run(
                    &uri,
                    &database,
                    conn_id,
                    Some(ProgressSink {
                        jobs: &state_cloned.jobs,
                        job_id,
                    }),
                )
                .await;
            let (status, job_result) = match &outcome {
                Ok(path) => {
                    tracing::info!(file = %path.display(), "scheduled backup ok");
                    if let Err(e) = state_cloned.runner.prune(conn_id, &database, retention).await {
                        tracing::warn!(error = %e, "retention prune failed");
                    }
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("ok")
                        .to_string();
                    ("ok".to_string(), Ok(name))
                }
                Err(e) => {
                    tracing::error!(error = %e, "scheduled backup failed");
                    (format!("error: {e}"), Err(e.to_string()))
                }
            };
            state_cloned.jobs.finish(job_id, job_result).await;
            if let Err(e) = state_cloned
                .schedules
                .record_run(schedule_id, started, status)
                .await
            {
                tracing::warn!(error = %e, "failed to record schedule run");
            }
        });
    }
    Ok(())
}
