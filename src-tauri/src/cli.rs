use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "handy", about = "Handy - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to the running instance, or starts
    /// recording once Handy has launched)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to the running
    /// instance, or starts recording once Handy has launched)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to the running instance; does
    /// nothing if Handy isn't running yet)
    #[arg(long)]
    pub cancel: bool,

    /// Write the next recording's transcription to this file instead of the
    /// one in settings (sent to the running instance, or applied once Handy
    /// has launched). Lasts for one recording; settings are not changed.
    #[arg(long, value_name = "FILE")]
    pub output_file: Option<PathBuf>,

    /// Start recording, never stop it (sent to the running instance, or
    /// starts recording once Handy has launched). Does nothing while a
    /// recording is already running.
    #[arg(
        long,
        conflicts_with_all = ["toggle_transcription", "toggle_post_process", "cancel"]
    )]
    pub start_transcription: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Transcribe this WAV (16 kHz mono) headlessly and exit. Runs the same
    /// batch transcription path as the app — no mic, no silence removal, no
    /// download (the model must already be installed). Long audio is still
    /// chunked at pauses, as in the app.
    #[arg(short = 'f', long, value_name = "WAV")]
    pub transcribe_file: Option<PathBuf>,

    /// Model id to load for --transcribe-file (default: the selected model).
    #[arg(long)]
    pub model: Option<String>,

    /// Hard-select the compute device for --transcribe-file by its registry
    /// index (see --list-devices). Omit to use the persisted accelerator
    /// setting. transcribe-cpp (whisper-family) models only.
    #[arg(long, value_name = "N")]
    pub device_index: Option<usize>,

    /// List the transcribe-cpp compute devices (with indices) and exit.
    #[arg(long)]
    pub list_devices: bool,

    /// List the available models (with ids) and exit. Pass an id to --model.
    /// Honors --json for machine-readable output.
    #[arg(long)]
    pub list_models: bool,

    /// Repeat the transcription N times (best_ms reports the fastest run).
    #[arg(long, value_name = "N")]
    pub repeat: Option<usize>,

    /// Emit --transcribe-file results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Resolves `--output-file` against the caller's working directory, so a
/// relative path means what it meant in the shell that ran the command.
pub fn resolve_output_file(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_output_file_and_start() {
        let args = CliArgs::try_parse_from([
            "handy",
            "--output-file",
            "C:\\t\\a.txt",
            "--start-transcription",
        ])
        .unwrap();
        assert_eq!(args.output_file, Some(PathBuf::from("C:\\t\\a.txt")));
        assert!(args.start_transcription);
    }

    #[test]
    fn start_conflicts_with_toggle_and_cancel() {
        for other in [
            "--toggle-transcription",
            "--toggle-post-process",
            "--cancel",
        ] {
            assert!(
                CliArgs::try_parse_from(["handy", "--start-transcription", other]).is_err(),
                "{other} should conflict"
            );
        }
    }

    #[test]
    fn relative_output_file_is_joined_to_cwd() {
        let cwd = std::env::temp_dir();
        assert_eq!(
            resolve_output_file(Path::new("t/a.txt"), &cwd),
            cwd.join("t/a.txt")
        );
    }

    #[test]
    fn absolute_output_file_is_kept() {
        let abs = std::env::temp_dir().join("a.txt");
        assert_eq!(resolve_output_file(&abs, Path::new("/work")), abs);
    }
}
