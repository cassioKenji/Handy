//! Output to a text file: the transcription is appended to a file the user
//! picked, instead of being pasted into the active application.

use crate::settings::AppSettings;
use log::error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The file a recording writes to as it goes, if the user set that up.
pub fn live_file_path(settings: &AppSettings) -> Option<PathBuf> {
    match destination(settings) {
        Destination::File(path) => Some(path),
        Destination::Paste | Destination::NoFile => None,
    }
}

/// Writes each decoded chunk of one recording to the file the moment it is
/// ready, so the file grows while the user is still talking. After a failure
/// it stops writing, since the user has already been told.
pub struct LiveFileSink {
    path: PathBuf,
    on_error: Box<dyn Fn(&std::io::Error) + Send + Sync>,
    wrote: AtomicUsize,
    failed: AtomicBool,
}

impl LiveFileSink {
    pub fn new(path: PathBuf, on_error: Box<dyn Fn(&std::io::Error) + Send + Sync>) -> Self {
        Self {
            path,
            on_error,
            wrote: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
        }
    }

    /// Adds `text` as a line. Blank text is skipped: a pause with no speech
    /// must not leave an empty line.
    pub fn write(&self, text: &str) {
        let text = text.trim();
        if text.is_empty() || self.failed.load(Ordering::Acquire) {
            return;
        }
        match append(&self.path, text) {
            Ok(()) => {
                self.wrote.fetch_add(1, Ordering::AcqRel);
            }
            Err(e) => {
                error!("Failed to write a live chunk to {:?}: {}", self.path, e);
                self.failed.store(true, Ordering::Release);
                (self.on_error)(&e);
            }
        }
    }

    /// Whether any chunk reached the file. When one did, the end of the
    /// recording must not write the whole text again.
    pub fn wrote_any(&self) -> bool {
        self.wrote.load(Ordering::Acquire) > 0
    }

    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }
}

/// Where a finished transcription goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Destination {
    /// Into the active application, as always.
    Paste,
    File(PathBuf),
    /// The switch is on but no file was ever picked. Pasting would hide that
    /// from the user, so it is reported instead.
    NoFile,
}

pub fn destination(settings: &AppSettings) -> Destination {
    if !settings.output_to_file_enabled {
        return Destination::Paste;
    }
    match settings.output_file_path.as_deref().map(str::trim) {
        Some(path) if !path.is_empty() => Destination::File(PathBuf::from(path)),
        _ => Destination::NoFile,
    }
}

/// Appends `text` and a line break to the file at `path`, creating the file
/// (but not its folder) when it doesn't exist yet. Existing content is kept.
pub fn append(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    // One write, so two transcriptions can't interleave inside a line.
    file.write_all(format!("{text}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("handy-output-file-{}-{name}", std::process::id()))
    }

    fn settings(enabled: bool, path: Option<&str>) -> AppSettings {
        let mut settings = crate::settings::get_default_settings();
        settings.output_to_file_enabled = enabled;
        settings.output_file_path = path.map(str::to_string);
        settings
    }

    #[test]
    fn pastes_unless_the_switch_is_on() {
        assert_eq!(
            destination(&settings(false, Some("notes.txt"))),
            Destination::Paste
        );
    }

    #[test]
    fn writes_to_the_picked_file_when_on() {
        assert_eq!(
            destination(&settings(true, Some("C:\\notes.txt"))),
            Destination::File(PathBuf::from("C:\\notes.txt"))
        );
    }

    #[test]
    fn reports_a_missing_file_instead_of_pasting() {
        assert_eq!(destination(&settings(true, None)), Destination::NoFile);
        assert_eq!(
            destination(&settings(true, Some("  "))),
            Destination::NoFile
        );
    }

    fn quiet_sink(path: &Path) -> LiveFileSink {
        LiveFileSink::new(path.to_path_buf(), Box::new(|_| {}))
    }

    #[test]
    fn live_sink_writes_each_chunk_as_its_own_line() {
        let path = temp_file("live.txt");
        let _ = std::fs::remove_file(&path);
        let sink = quiet_sink(&path);

        sink.write("  first chunk ");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first chunk\n");
        sink.write("second chunk");

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "first chunk\nsecond chunk\n"
        );
        assert!(sink.wrote_any() && !sink.failed());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn live_sink_skips_blank_chunks() {
        let path = temp_file("live-blank.txt");
        let _ = std::fs::remove_file(&path);
        let sink = quiet_sink(&path);

        sink.write("   ");
        sink.write("");

        assert!(!sink.wrote_any());
        assert!(!path.exists());
    }

    #[test]
    fn live_sink_reports_a_failure_once_and_then_stops() {
        let path = temp_file("live-missing-folder").join("out.txt");
        let reports = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&reports);
        let sink = LiveFileSink::new(
            path,
            Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        );

        sink.write("one");
        sink.write("two");

        assert!(sink.failed() && !sink.wrote_any());
        assert_eq!(reports.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn live_file_path_is_only_set_when_writing_to_a_file() {
        assert_eq!(live_file_path(&settings(false, Some("a.txt"))), None);
        assert_eq!(live_file_path(&settings(true, None)), None);
        assert_eq!(
            live_file_path(&settings(true, Some("a.txt"))),
            Some(PathBuf::from("a.txt"))
        );
    }

    #[test]
    fn creates_the_file_and_adds_one_line_per_transcription() {
        let path = temp_file("create.txt");
        let _ = std::fs::remove_file(&path);

        append(&path, "first").unwrap();
        append(&path, "second").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn keeps_what_the_file_already_had() {
        let path = temp_file("existing.txt");
        std::fs::write(&path, "old line\n").unwrap();

        append(&path, "new line").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "old line\nnew line\n"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn keeps_non_ascii_text_intact() {
        let path = temp_file("utf8.txt");
        let _ = std::fs::remove_file(&path);

        append(&path, "olá, transcrição").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "olá, transcrição\n"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fails_when_the_folder_does_not_exist() {
        let path = temp_file("missing-folder").join("out.txt");
        assert!(append(&path, "text").is_err());
    }
}
