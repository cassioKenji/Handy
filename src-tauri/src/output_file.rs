//! Output to a text file: the transcription is appended to a file the user
//! picked, instead of being pasted into the active application.

use crate::settings::AppSettings;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

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
