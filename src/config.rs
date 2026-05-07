use anyhow::{anyhow, bail};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: String,
    pub port: u16,
    pub session_key: Vec<u8>,
    pub data_dir: PathBuf,
    pub backup_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".into());
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse()
            .map_err(|e| anyhow!("invalid PORT: {e}"))?;

        let session_key_raw = std::env::var("SESSION_KEY")
            .map_err(|_| anyhow!("SESSION_KEY is required (>= 64 bytes of random data)"))?;
        let session_key = decode_session_key(&session_key_raw);
        if session_key.len() < 64 {
            bail!(
                "SESSION_KEY must decode to at least 64 bytes (got {})",
                session_key.len()
            );
        }

        let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".into()));
        let backup_dir =
            PathBuf::from(std::env::var("BACKUP_DIR").unwrap_or_else(|_| "./backups".into()));

        Ok(Self {
            bind_addr,
            port,
            session_key,
            data_dir,
            backup_dir,
        })
    }
}

fn decode_session_key(raw: &str) -> Vec<u8> {
    if let Some(bytes) = hex_decode(raw) {
        return bytes;
    }
    raw.as_bytes().to_vec()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 2 != 0 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
