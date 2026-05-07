use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::VecDeque;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

const MAX_HISTORY: usize = 50;
const EVENT_BUFFER: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Ok,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobSource {
    Manual,
    Scheduled,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct JobProgress {
    pub current_collection: Option<String>,
    pub collections_done: u32,
    pub last_log: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub connection_label: String,
    pub database: String,
    pub source: JobSource,
    pub state: JobState,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
    pub progress: JobProgress,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Snapshot { jobs: Vec<Job> },
    Started { job: Job },
    Progress { job: Job },
    Finished { job: Job },
}

pub struct JobRegistry {
    jobs: RwLock<VecDeque<Job>>,
    tx: broadcast::Sender<JobEvent>,
}

impl JobRegistry {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            jobs: RwLock::new(VecDeque::new()),
            tx,
        }
    }

    pub async fn list(&self) -> Vec<Job> {
        self.jobs.read().await.iter().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.tx.subscribe()
    }

    pub async fn start(
        &self,
        connection_id: Uuid,
        connection_label: String,
        database: String,
        source: JobSource,
    ) -> Uuid {
        let job = Job {
            id: Uuid::new_v4(),
            connection_id,
            connection_label,
            database,
            source,
            state: JobState::Running,
            started_at: Utc::now(),
            finished_at: None,
            message: None,
            progress: JobProgress::default(),
        };
        let id = job.id;
        {
            let mut guard = self.jobs.write().await;
            guard.push_front(job.clone());
            trim(&mut guard);
        }
        let _ = self.tx.send(JobEvent::Started { job });
        id
    }

    /// Mutate the job's progress and broadcast a Progress event.
    /// No-op if the job no longer exists.
    pub async fn update_progress<F>(&self, id: Uuid, mutate: F)
    where
        F: FnOnce(&mut JobProgress),
    {
        let mut snapshot = None;
        {
            let mut guard = self.jobs.write().await;
            if let Some(job) = guard.iter_mut().find(|j| j.id == id) {
                if job.state == JobState::Running {
                    mutate(&mut job.progress);
                    snapshot = Some(job.clone());
                }
            }
        }
        if let Some(job) = snapshot {
            let _ = self.tx.send(JobEvent::Progress { job });
        }
    }

    pub async fn finish(&self, id: Uuid, result: Result<String, String>) {
        let mut snapshot = None;
        {
            let mut guard = self.jobs.write().await;
            if let Some(job) = guard.iter_mut().find(|j| j.id == id) {
                job.finished_at = Some(Utc::now());
                match result {
                    Ok(msg) => {
                        job.state = JobState::Ok;
                        job.message = Some(msg);
                    }
                    Err(msg) => {
                        job.state = JobState::Failed;
                        job.message = Some(msg);
                    }
                }
                snapshot = Some(job.clone());
            }
        }
        if let Some(job) = snapshot {
            let _ = self.tx.send(JobEvent::Finished { job });
        }
    }
}

fn trim(jobs: &mut VecDeque<Job>) {
    // Keep all running jobs + up to MAX_HISTORY finished jobs (newest first).
    let mut finished_kept = 0;
    let mut idx = 0;
    while idx < jobs.len() {
        match jobs[idx].state {
            JobState::Running => idx += 1,
            JobState::Ok | JobState::Failed => {
                if finished_kept < MAX_HISTORY {
                    finished_kept += 1;
                    idx += 1;
                } else {
                    jobs.remove(idx);
                }
            }
        }
    }
}
