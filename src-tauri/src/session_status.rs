//! `recording.json` in the app data folder: present while a recording
//! session (recording plus its processing) runs, so scripts can tell
//! whether Handy is busy and which file it writes to.

use crate::managers::audio::AudioRecordingManager;
use crate::output_file::{self, Destination};
use crate::settings::{get_settings, InputSource};
use log::{debug, warn};
use serde::Serialize;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

const FILE_NAME: &str = "recording.json";

#[derive(Debug, Serialize, PartialEq)]
struct RecordingStatus {
    recording: bool,
    file: Option<String>,
    since: String,
    source: InputSource,
    device: Option<String>,
    pid: u32,
}

fn status_path(app: &AppHandle) -> Option<PathBuf> {
    match crate::portable::app_data_dir(app) {
        Ok(dir) => Some(dir.join(FILE_NAME)),
        Err(e) => {
            warn!("No app data folder for {FILE_NAME}: {e}");
            None
        }
    }
}

/// Writes through a temporary file and a rename, so a reader never sees
/// half a JSON document.
fn write_to(path: &Path, status: &RecordingStatus) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(status).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

fn remove_at(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Called once a recording has really started.
pub fn write_started(app: &AppHandle) {
    let Some(path) = status_path(app) else {
        return;
    };
    let settings = get_settings(app);
    let file = match output_file::destination(&settings) {
        Destination::File(file) => {
            // Create the transcript up front, so a watcher sees the file
            // before the first words arrive. Existing content is kept.
            if let Err(e) = OpenOptions::new().create(true).append(true).open(&file) {
                warn!("Could not create {:?}: {e}", file);
            }
            Some(file.display().to_string())
        }
        Destination::Paste | Destination::NoFile => None,
    };
    let source = match app.try_state::<Arc<AudioRecordingManager>>() {
        Some(rm) if rm.capturing_system_output() => InputSource::SystemOutput,
        _ => InputSource::Microphone,
    };
    let device = match source {
        InputSource::SystemOutput => settings.selected_loopback_device.clone(),
        InputSource::Microphone => settings.selected_microphone.clone(),
    };
    let status = RecordingStatus {
        recording: true,
        file,
        since: chrono::Local::now().to_rfc3339(),
        source,
        device,
        pid: std::process::id(),
    };
    match write_to(&path, &status) {
        Ok(()) => debug!("Wrote {:?}", path),
        Err(e) => warn!("Could not write {:?}: {e}", path),
    }
}

/// Called when the session ends, at startup (a crash may have left the
/// file behind) and on exit.
pub fn clear(app: &AppHandle) {
    let Some(path) = status_path(app) else {
        return;
    };
    if let Err(e) = remove_at(&path) {
        warn!("Could not remove {:?}: {e}", path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RecordingStatus {
        RecordingStatus {
            recording: true,
            file: Some("C:\\t\\daily.txt".into()),
            since: "2026-11-26T09:30:12-03:00".into(),
            source: InputSource::SystemOutput,
            device: None,
            pid: 42,
        }
    }

    #[test]
    fn serializes_the_documented_shape() {
        let json: serde_json::Value = serde_json::to_value(sample()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "recording": true,
                "file": "C:\\t\\daily.txt",
                "since": "2026-11-26T09:30:12-03:00",
                "source": "system_output",
                "device": null,
                "pid": 42
            })
        );
    }

    #[test]
    fn write_then_remove() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        write_to(&path, &sample()).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        remove_at(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn removing_a_missing_file_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        remove_at(&dir.path().join(FILE_NAME)).unwrap();
    }
}
