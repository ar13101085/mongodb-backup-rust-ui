use crate::backup::jobs::JobRegistry;
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const ARCHIVE_EXT: &str = ".archive.gz";
const MAX_STDERR_TAIL: usize = 200;
const MAX_LOG_LINE: usize = 240;

#[derive(Debug, Clone, Serialize)]
pub struct BackupFile {
    pub filename: String,
    pub connection_id: Option<Uuid>,
    pub database: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub size_bytes: u64,
}

pub struct ProgressSink<'a> {
    pub jobs: &'a JobRegistry,
    pub job_id: Uuid,
}

pub struct BackupRunner {
    backup_dir: PathBuf,
}

impl BackupRunner {
    pub fn new(backup_dir: PathBuf) -> Self {
        Self { backup_dir }
    }

    pub async fn ensure_dir(&self) -> AppResult<()> {
        fs::create_dir_all(&self.backup_dir).await?;
        Ok(())
    }

    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    /// Run mongodump against `uri`/`database`, writing a single gzipped archive.
    /// Returns the absolute path to the produced archive on success.
    /// If `progress` is provided, parses mongodump's stderr live and pushes updates to the sink.
    pub async fn run(
        &self,
        uri: &str,
        database: &str,
        connection_id: Uuid,
        progress: Option<ProgressSink<'_>>,
    ) -> AppResult<PathBuf> {
        self.ensure_dir().await?;
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let safe_db = sanitize(database);
        let filename = format!("{connection_id}__{safe_db}__{timestamp}{ARCHIVE_EXT}");
        let path = self.backup_dir.join(&filename);

        let archive_arg = format!("--archive={}", path.display());
        let uri_arg = format!("--uri={uri}");
        let db_arg = format!("--db={database}");

        tracing::info!(target = %database, file = %filename, "starting mongodump");
        let mut child = Command::new("mongodump")
            .arg(&uri_arg)
            .arg(&db_arg)
            .arg(&archive_arg)
            .arg("--gzip")
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Internal(format!("failed to spawn mongodump: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Internal("mongodump stderr unavailable".into()))?;
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::with_capacity(MAX_STDERR_TAIL);

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| AppError::Internal(format!("read mongodump stderr: {e}")))?
        {
            if let Some(sink) = &progress {
                apply_progress(sink, &line).await;
            }
            if tail.len() == MAX_STDERR_TAIL {
                tail.remove(0);
            }
            tail.push(line);
        }

        let status = child
            .wait()
            .await
            .map_err(|e| AppError::Internal(format!("waiting on mongodump: {e}")))?;

        if !status.success() {
            let _ = fs::remove_file(&path).await;
            let stderr = tail
                .iter()
                .rev()
                .take(10)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(AppError::Internal(format!(
                "mongodump failed (exit {:?}): {}",
                status.code(),
                stderr
            )));
        }
        Ok(path)
    }

    /// Run mongorestore from a single gzipped archive.
    pub async fn restore(
        &self,
        uri: &str,
        archive_path: &Path,
        target_database: Option<&str>,
        drop_existing: bool,
    ) -> AppResult<()> {
        if !fs::try_exists(archive_path).await? {
            return Err(AppError::NotFound);
        }
        let archive_arg = format!("--archive={}", archive_path.display());
        let uri_arg = format!("--uri={uri}");

        let mut cmd = Command::new("mongorestore");
        cmd.arg(&uri_arg).arg(&archive_arg).arg("--gzip");
        if drop_existing {
            cmd.arg("--drop");
        }
        if let Some(db) = target_database {
            cmd.arg(format!("--nsInclude={db}.*"));
        }

        tracing::info!(archive = %archive_path.display(), "starting mongorestore");
        let output = cmd
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("failed to spawn mongorestore: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Internal(format!(
                "mongorestore failed (exit {:?}): {}",
                output.status.code(),
                stderr.trim()
            )));
        }
        Ok(())
    }

    /// List archives in the backup directory, parsing filenames for metadata.
    pub async fn list(&self) -> AppResult<Vec<BackupFile>> {
        self.ensure_dir().await?;
        let mut entries = fs::read_dir(&self.backup_dir).await?;
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(ARCHIVE_EXT) {
                continue;
            }
            let meta = entry.metadata().await?;
            if !meta.is_file() {
                continue;
            }
            let (conn_id, db, created) = parse_filename(name);
            out.push(BackupFile {
                filename: name.to_string(),
                connection_id: conn_id,
                database: db,
                created_at: created,
                size_bytes: meta.len(),
            });
        }
        out.sort_by(|a, b| b.filename.cmp(&a.filename));
        Ok(out)
    }

    pub async fn delete(&self, filename: &str) -> AppResult<()> {
        let path = self.resolve(filename)?;
        fs::remove_file(&path).await?;
        Ok(())
    }

    /// Resolve a filename to an absolute path inside backup_dir, rejecting traversal.
    pub fn resolve(&self, filename: &str) -> AppResult<PathBuf> {
        if filename.is_empty() || filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err(AppError::BadRequest("invalid filename".into()));
        }
        if !filename.ends_with(ARCHIVE_EXT) {
            return Err(AppError::BadRequest("not an archive filename".into()));
        }
        Ok(self.backup_dir.join(filename))
    }

    /// Keep the newest `retention` backups for (connection_id, database); delete the rest.
    pub async fn prune(
        &self,
        connection_id: Uuid,
        database: &str,
        retention: u32,
    ) -> AppResult<usize> {
        let all = self.list().await?;
        let mut matching: Vec<_> = all
            .into_iter()
            .filter(|b| b.connection_id == Some(connection_id) && b.database.as_deref() == Some(database))
            .collect();
        matching.sort_by(|a, b| b.filename.cmp(&a.filename));
        let mut deleted = 0;
        for old in matching.into_iter().skip(retention as usize) {
            if let Err(e) = self.delete(&old.filename).await {
                tracing::warn!(file = %old.filename, error = %e, "prune delete failed");
            } else {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

#[derive(Debug)]
enum ParsedLine<'a> {
    Writing(&'a str),
    Done,
}

/// Parse a single line of mongodump stderr looking for collection-level progress markers.
/// mongodump format examples:
///   "2026-... INFO\twriting mydb.users to archive 'foo'"
///   "2026-... INFO\tdone dumping mydb.users (4218 documents)"
///   "2026-... INFO\tdone dumping mydb (5 collections)"   <-- database-level summary; ignored
fn parse_line(line: &str) -> Option<ParsedLine<'_>> {
    if let Some(idx) = line.find("writing ") {
        let rest = &line[idx + "writing ".len()..];
        if let Some((name, _)) = rest.split_once(" to ") {
            let trimmed = name.trim();
            if trimmed.contains('.') {
                return Some(ParsedLine::Writing(trimmed));
            }
        }
    }
    if let Some(idx) = line.find("done dumping ") {
        let rest = &line[idx + "done dumping ".len()..];
        let name = rest.split_once(" (").map(|(n, _)| n).unwrap_or(rest).trim();
        // Collection messages have the form "db.coll"; the "db only" message at the end
        // (e.g. "done dumping mydb (5 collections)") has no dot — skip it.
        if name.contains('.') {
            return Some(ParsedLine::Done);
        }
    }
    None
}

async fn apply_progress(sink: &ProgressSink<'_>, line: &str) {
    let trimmed_log = if line.len() > MAX_LOG_LINE {
        &line[line.len() - MAX_LOG_LINE..]
    } else {
        line
    };
    let log_owned = trimmed_log.to_string();

    match parse_line(line) {
        Some(ParsedLine::Writing(name)) => {
            let name = name.to_string();
            sink.jobs
                .update_progress(sink.job_id, |p| {
                    p.current_collection = Some(name);
                    p.last_log = Some(log_owned);
                })
                .await;
        }
        Some(ParsedLine::Done) => {
            sink.jobs
                .update_progress(sink.job_id, |p| {
                    p.collections_done = p.collections_done.saturating_add(1);
                    p.current_collection = None;
                    p.last_log = Some(log_owned);
                })
                .await;
        }
        None => {
            // Still update last_log for visibility, but throttle to avoid flooding events
            // by only updating on lines that look like meaningful status changes.
            if line.contains("dumping") || line.contains("error") || line.contains("Failed") {
                sink.jobs
                    .update_progress(sink.job_id, |p| {
                        p.last_log = Some(log_owned);
                    })
                    .await;
            }
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn parse_filename(name: &str) -> (Option<Uuid>, Option<String>, Option<DateTime<Utc>>) {
    let stem = name.strip_suffix(ARCHIVE_EXT).unwrap_or(name);
    let parts: Vec<&str> = stem.split("__").collect();
    if parts.len() != 3 {
        return (None, None, None);
    }
    let id = Uuid::parse_str(parts[0]).ok();
    let db = Some(parts[1].to_string());
    let ts = chrono::NaiveDateTime::parse_from_str(parts[2], "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|dt| dt.and_utc());
    (id, db, ts)
}
