use crate::error::{AppError, AppResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::{RwLock, RwLockReadGuard};

pub mod connections;
pub mod schedules;
pub mod users;

/// JSON file store with in-memory cache and atomic writes.
pub struct JsonFile<T> {
    path: PathBuf,
    data: RwLock<T>,
}

impl<T> JsonFile<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub async fn load_or_init(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = if fs::try_exists(&path).await? {
            let raw = fs::read(&path).await?;
            if raw.is_empty() {
                T::default()
            } else {
                serde_json::from_slice(&raw)?
            }
        } else {
            let value = T::default();
            write_atomic(&path, &value).await?;
            value
        };
        Ok(Self {
            path,
            data: RwLock::new(data),
        })
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.data.read().await
    }

    /// Apply `f` to the in-memory data, then atomically persist the result.
    /// If serialization or disk write fails, the in-memory data is rolled back.
    pub async fn update<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&mut T) -> R,
        T: Clone,
    {
        let mut guard = self.data.write().await;
        let backup = guard.clone();
        let result = f(&mut *guard);
        if let Err(e) = write_atomic(&self.path, &*guard).await {
            *guard = backup;
            return Err(e);
        }
        Ok(result)
    }

    /// Apply `f` and persist only if `f` returns Ok.
    pub async fn try_update<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce(&mut T) -> AppResult<R>,
        T: Clone,
    {
        let mut guard = self.data.write().await;
        let backup = guard.clone();
        let result = match f(&mut *guard) {
            Ok(v) => v,
            Err(e) => {
                *guard = backup;
                return Err(e);
            }
        };
        if let Err(e) = write_atomic(&self.path, &*guard).await {
            *guard = backup;
            return Err(e);
        }
        Ok(result)
    }
}

async fn write_atomic<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).await?;
    fs::rename(&tmp, path)
        .await
        .map_err(|e| AppError::Io(std::io::Error::new(e.kind(), format!("rename failed: {e}"))))?;
    Ok(())
}
