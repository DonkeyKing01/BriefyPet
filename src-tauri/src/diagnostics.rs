use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use chrono::Utc;
use tauri::AppHandle;

const DIAGNOSTIC_LOG_NAME: &str = "fetch-diagnostic.log";

fn diagnostic_log_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path_resolver().app_data_dir()?;
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir.join(DIAGNOSTIC_LOG_NAME))
}

pub fn log(app: &AppHandle, scope: &str, message: impl AsRef<str>) {
    let Some(path) = diagnostic_log_path(app) else {
        return;
    };

    let timestamp = Utc::now().to_rfc3339();
    let line = format!("[{timestamp}] [{scope}] {}\n", message.as_ref());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

