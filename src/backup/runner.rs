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
        let stripped_uri = strip_uri_path_db(uri);
        let uri_arg = format!("--uri={stripped_uri}");
        let db_arg = format!("--db={database}");

        tracing::info!(target = %database, file = %filename, "starting mongodump");
        let mut cmd = Command::new("mongodump");
        cmd.arg(&uri_arg)
            .arg(&db_arg)
            .arg(&archive_arg)
            .arg("--gzip");
        // When --db is non-admin and the URI doesn't specify authSource, mongodump
        // negotiates SCRAM mechanisms against the dump database (which doesn't host
        // the user) and falls back to SHA-1 — failing for SHA-256-only users. Default
        // the auth database to admin if the user hasn't picked one in the URI.
        if !uri_has_query_param(uri, "authSource") {
            cmd.arg("--authenticationDatabase=admin");
        }
        let mut child = cmd
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

    /// Run mongorestore from a single gzipped archive. Streams stderr line-by-line,
    /// pushing per-collection progress to `progress` when present.
    pub async fn restore(
        &self,
        uri: &str,
        archive_path: &Path,
        target_database: Option<&str>,
        drop_existing: bool,
        progress: Option<ProgressSink<'_>>,
    ) -> AppResult<()> {
        if !fs::try_exists(archive_path).await? {
            return Err(AppError::NotFound);
        }
        let archive_arg = format!("--archive={}", archive_path.display());
        let stripped_uri = strip_uri_path_db(uri);
        let uri_arg = format!("--uri={stripped_uri}");

        let mut cmd = Command::new("mongorestore");
        cmd.arg(&uri_arg).arg(&archive_arg).arg("--gzip");
        if !uri_has_query_param(uri, "authSource") {
            cmd.arg("--authenticationDatabase=admin");
        }
        if drop_existing {
            cmd.arg("--drop");
        }
        // --db on a single-database archive restores into that name, regardless of
        // the original db name embedded in the archive. (We always produce single-db
        // archives via `mongodump --db=...`.) NOTE: --nsInclude was wrong here — it
        // is a *filter*, not a rename, and would silently restore nothing when the
        // requested name differed from the archive's source db.
        if let Some(db) = target_database {
            cmd.arg(format!("--db={db}"));
        }

        tracing::info!(archive = %archive_path.display(), "starting mongorestore");
        let mut child = cmd
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| AppError::Internal(format!("failed to spawn mongorestore: {e}")))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Internal("mongorestore stderr unavailable".into()))?;
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::with_capacity(MAX_STDERR_TAIL);

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| AppError::Internal(format!("read mongorestore stderr: {e}")))?
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
            .map_err(|e| AppError::Internal(format!("waiting on mongorestore: {e}")))?;

        if !status.success() {
            let stderr = tail
                .iter()
                .rev()
                .take(10)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join(" | ");
            return Err(AppError::Internal(format!(
                "mongorestore failed (exit {:?}): {}",
                status.code(),
                stderr
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

/// Parse a single line of mongodump or mongorestore stderr for collection-level
/// progress markers. Examples:
///   mongodump:    "writing mydb.users to archive 'foo'"
///                 "done dumping mydb.users (4218 documents)"
///                 "done dumping mydb (5 collections)"        <- db-level summary; ignored
///   mongorestore: "restoring mydb.users from archive 'foo'"
///                 "finished restoring mydb.users (4218 documents, 0 failures)"
fn parse_line(line: &str) -> Option<ParsedLine<'_>> {
    // Done patterns must be checked before Started ones, because
    // "finished restoring X" contains "restoring " as a substring.
    if let Some(idx) = line.find("done dumping ") {
        let rest = &line[idx + "done dumping ".len()..];
        let name = rest.split_once(" (").map(|(n, _)| n).unwrap_or(rest).trim();
        if name.contains('.') {
            return Some(ParsedLine::Done);
        }
    }
    if let Some(idx) = line.find("finished restoring ") {
        let rest = &line[idx + "finished restoring ".len()..];
        let name = rest.split_once(" (").map(|(n, _)| n).unwrap_or(rest).trim();
        if name.contains('.') {
            return Some(ParsedLine::Done);
        }
    }
    if let Some(idx) = line.find("writing ") {
        let rest = &line[idx + "writing ".len()..];
        if let Some((name, _)) = rest.split_once(" to ") {
            let trimmed = name.trim();
            if trimmed.contains('.') {
                return Some(ParsedLine::Writing(trimmed));
            }
        }
    }
    if let Some(idx) = line.find("restoring ") {
        let rest = &line[idx + "restoring ".len()..];
        if let Some((name, _)) = rest.split_once(" from") {
            let trimmed = name.trim();
            if trimmed.contains('.') {
                return Some(ParsedLine::Writing(trimmed));
            }
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
            if line.contains("dumping")
                || line.contains("restoring")
                || line.contains("restore")
                || line.contains("error")
                || line.contains("Failed")
            {
                sink.jobs
                    .update_progress(sink.job_id, |p| {
                        p.last_log = Some(log_owned);
                    })
                    .await;
            }
        }
    }
}

/// Case-insensitive lookup for a query-string key in a MongoDB URI.
fn uri_has_query_param(uri: &str, key: &str) -> bool {
    let Some((_, query)) = uri.split_once('?') else { return false };
    query.split('&').any(|kv| {
        kv.split_once('=')
            .map(|(k, _)| k.eq_ignore_ascii_case(key))
            .unwrap_or(false)
    })
}

/// Remove the database path component from a MongoDB URI, preserving query params.
/// `mongodb://u:p@h:port/dbname?x=y` → `mongodb://u:p@h:port/?x=y`.
/// Without this, mongodump/mongorestore reject configurations where the caller also
/// passes `--db=<other>` (they consider it a conflicting database directive).
fn strip_uri_path_db(uri: &str) -> String {
    let Some(scheme_end) = uri.find("://") else { return uri.to_string() };
    let prefix = &uri[..scheme_end + 3];
    let rest = &uri[scheme_end + 3..];
    let split = rest.find(|c| c == '/' || c == '?');
    match split {
        None => uri.to_string(),
        Some(idx) => {
            let auth = &rest[..idx];
            let after = &rest[idx..];
            let query = after.find('?').map(|i| &after[i..]).unwrap_or("");
            format!("{prefix}{auth}/{query}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_path_db_keeps_query() {
        assert_eq!(
            strip_uri_path_db("mongodb://u:p@h:8081/foo?authSource=admin"),
            "mongodb://u:p@h:8081/?authSource=admin"
        );
    }

    #[test]
    fn no_path_left_alone() {
        assert_eq!(
            strip_uri_path_db("mongodb://u:p@h:8081"),
            "mongodb://u:p@h:8081"
        );
    }

    #[test]
    fn no_path_with_query_left_alone() {
        assert_eq!(
            strip_uri_path_db("mongodb://u:p@h:8081?w=majority"),
            "mongodb://u:p@h:8081/?w=majority"
        );
    }

    #[test]
    fn srv_form_works() {
        assert_eq!(
            strip_uri_path_db("mongodb+srv://u:p@cluster.mongodb.net/mydb?retryWrites=true"),
            "mongodb+srv://u:p@cluster.mongodb.net/?retryWrites=true"
        );
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
