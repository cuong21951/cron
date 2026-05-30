//! Persistence for the crontab: a small JSON file holding the list of jobs.
//!
//! On Windows the file lives at `%APPDATA%\cron\crontab.json`; on other
//! platforms it falls back to `$HOME/.config/cron/crontab.json`. Override the
//! location with the `CRON_HOME` environment variable.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::schedule::Schedule;

/// A single scheduled job: a cron expression plus the command to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// The raw cron expression, e.g. `"*/5 * * * *"`.
    pub schedule: String,
    /// The shell command to execute when the job fires.
    pub command: String,
}

impl Job {
    /// Parse this job's cron expression.
    pub fn parsed_schedule(&self) -> Result<Schedule, String> {
        Schedule::parse(&self.schedule)
    }
}

/// The on-disk crontab.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Crontab {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

impl Crontab {
    /// Load the crontab from disk, returning an empty one if no file exists.
    pub fn load() -> Result<Crontab, String> {
        let path = crontab_path()?;
        if !path.exists() {
            return Ok(Crontab::default());
        }
        let data = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        serde_json::from_str(&data).map_err(|e| format!("failed to parse {}: {e}", path.display()))
    }

    /// Write the crontab back to disk, creating the directory if needed.
    pub fn save(&self) -> Result<(), String> {
        let path = crontab_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize crontab: {e}"))?;
        fs::write(&path, data).map_err(|e| format!("failed to write {}: {e}", path.display()))
    }
}

/// Resolve the path to the crontab file.
pub fn crontab_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("crontab.json"))
}

/// Resolve the configuration directory holding the crontab.
pub fn config_dir() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("CRON_HOME") {
        return Ok(PathBuf::from(home));
    }

    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA").ok_or_else(|| "APPDATA is not set".to_string())?;
        Ok(PathBuf::from(base).join("cron"))
    }

    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg).join("cron"));
        }
        let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        Ok(PathBuf::from(home).join(".config").join("cron"))
    }
}
