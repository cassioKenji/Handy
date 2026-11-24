use crate::Error;
use crate::backend::{Backend, cpal::CpalBackend};
use crate::capture::engine::open_error;

/// Lists input devices. Callable any time, from any thread.
pub fn list_input_devices() -> Result<Vec<InputDevice>, Error> {
    CpalBackend::shared()
        .list_input_devices()
        .map_err(|e| open_error(e, None))
}

/// Lists outputs that can be recorded by loopback: what a sound card is
/// playing. Empty where the OS can't (macOS before 14.6, Linux). Pass an
/// entry's `id` as `RecorderConfig::device` to record it.
pub fn list_loopback_devices() -> Result<Vec<InputDevice>, Error> {
    CpalBackend::shared()
        .list_loopback_devices()
        .map_err(|e| open_error(e, None))
}

/// Whether this OS can record its output (Windows; macOS 14.6 or later).
pub fn loopback_supported() -> bool {
    CpalBackend::shared().loopback_supported()
}

/// What an [`InputDevice`] records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DeviceKind {
    /// A microphone or other input.
    #[default]
    Microphone,
    /// A sound card's output, recorded by loopback (WASAPI loopback on
    /// Windows, a CoreAudio process tap on macOS).
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InputDevice {
    /// Pass as `RecorderConfig::device` to open this device.
    pub id: String,
    pub name: String,
    /// Distinguishes devices with the same name: 0 for the first, 1 for the
    /// second, and so on.
    pub occurrence: u32,
    pub backend: String,
    pub is_default: bool,
    /// Whether `id` survives a restart or replug. False means best effort.
    pub id_is_stable: bool,
    /// Channels at the device's OS format, for `Channels::Only`. `None` where
    /// reading it would open the device (ALSA).
    pub channels: Option<u16>,
    /// A PulseAudio monitor source ("Monitor of ..."), not a microphone.
    /// Always false on macOS and Windows.
    pub is_monitor: bool,
    /// A microphone, or an output recorded by loopback.
    pub kind: DeviceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Permission {
    Granted,
    Denied,
    NotDetermined,
    /// No answer up front (Linux, or Windows settings that can't be read);
    /// denial surfaces from `open`.
    Unknown,
}

/// Microphone permission status. A synchronous read; never prompts.
pub fn permission_status() -> Permission {
    crate::backend::permission::permission_status()
}
