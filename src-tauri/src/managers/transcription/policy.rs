//! How audio is decoded: the one place the model's traits and the settings
//! choose between streaming, long-form chunking and a single run.

use crate::engine_supervisor::LoadedInfo;

/// What a model can do that decides how its audio is decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelTraits {
    /// Decodes audio incrementally as it is recorded.
    pub streaming: bool,
    /// Keeps its accuracy on audio longer than its native window in one run
    /// (whisper today).
    pub long_form: bool,
}

impl ModelTraits {
    /// The loaded model's own traits: the authority once a model is loaded.
    pub fn of(info: &LoadedInfo) -> Self {
        Self {
            streaming: info.capabilities.supports_streaming,
            long_form: info.supports_long_form,
        }
    }
}

/// How a recording is decoded while it is still going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveDecode {
    /// The model decodes the audio as it arrives.
    Stream,
    /// Long audio is cut at pauses and each chunk decoded once its cut is
    /// certain.
    Chunked,
    /// Nothing until the recording stops; then it is decoded in batch.
    None,
}

/// How a whole clip is decoded: a recording without a live result, a
/// re-transcribed history entry, or a file from the CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchDecode {
    /// Cut at pauses into chunks of about 30 s, each decoded in turn; a clip
    /// of 40 s or less is one chunk.
    Chunked,
    /// One run.
    Whole,
}

/// Streaming wins: a streaming model handles long audio itself. Otherwise
/// long audio is chunked unless the model keeps its accuracy on it, and a
/// recording that writes to a file as it goes is chunked whatever the model:
/// the chunks are what reach the file while the user is still talking.
pub fn live_decode(model: ModelTraits, long_form_chunking: bool, live_to_file: bool) -> LiveDecode {
    if model.streaming {
        LiveDecode::Stream
    } else if live_to_file || (long_form_chunking && !model.long_form) {
        LiveDecode::Chunked
    } else {
        LiveDecode::None
    }
}

/// As live, except that a streaming model takes a whole clip in one run, as
/// it did before long-form chunking existed.
pub fn batch_decode(model: ModelTraits, long_form_chunking: bool) -> BatchDecode {
    if long_form_chunking && !model.long_form && !model.streaming {
        BatchDecode::Chunked
    } else {
        BatchDecode::Whole
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_FORM: ModelTraits = ModelTraits {
        streaming: false,
        long_form: true,
    };
    const STREAMING: ModelTraits = ModelTraits {
        streaming: true,
        long_form: false,
    };

    #[test]
    fn a_long_form_model_waits_for_the_stop_unless_writing_to_a_file() {
        assert_eq!(live_decode(LONG_FORM, true, false), LiveDecode::None);
        assert_eq!(live_decode(LONG_FORM, true, true), LiveDecode::Chunked);
    }

    #[test]
    fn writing_to_a_file_chunks_even_with_long_form_chunking_off() {
        assert_eq!(live_decode(LONG_FORM, false, true), LiveDecode::Chunked);
    }

    #[test]
    fn a_streaming_model_keeps_streaming_when_writing_to_a_file() {
        assert_eq!(live_decode(STREAMING, true, true), LiveDecode::Stream);
    }
}
