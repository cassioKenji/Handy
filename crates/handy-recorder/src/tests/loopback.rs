//! Loopback devices over the fake backend: listing, IDs, opening, and
//! microphone permission not applying to them.

use super::support::{self, Chunks, fake, open, start};
use crate::backend::Backend;
use crate::{DeviceKind, ErrorKind, Permission, Recorder, RecorderConfig};

fn loopback_config(raw_id: &str) -> RecorderConfig {
    RecorderConfig {
        device: Some(format!("loopback:{raw_id}")),
        frames_per_chunk: Some(160),
        ..RecorderConfig::default()
    }
}

#[test]
fn loopback_devices_are_listed_apart_from_microphones() {
    let fake = fake(16_000, 2);
    fake.add_loopback("spk", "Fake Speakers");
    let inputs = fake.list_input_devices().unwrap();
    let outputs = fake.list_loopback_devices().unwrap();
    assert!(inputs.iter().all(|d| d.kind == DeviceKind::Microphone));
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].kind, DeviceKind::Loopback);
    assert_eq!(outputs[0].id, "loopback:spk");
}

#[test]
fn a_loopback_id_opens_the_output_even_when_a_microphone_has_the_same_raw_id() {
    let fake = fake(16_000, 2);
    // The fake microphone's ID is "fake:mic"; give the output the same raw ID.
    fake.add_loopback("fake:mic", "Fake Speakers");
    let recorder: Recorder<Chunks> = open(&fake, loopback_config("fake:mic"));
    assert_eq!(recorder.info().device.kind, DeviceKind::Loopback);
    assert_eq!(recorder.info().device.name, "Fake Speakers");
}

#[test]
fn an_unknown_loopback_id_is_unavailable() {
    let fake = fake(16_000, 2);
    let error = support::open_with::<Chunks>(&fake, loopback_config("nope"), support::timeouts())
        .err()
        .expect("open should fail");
    assert_eq!(error.kind(), ErrorKind::DeviceUnavailable);
}

#[test]
fn without_loopback_support_the_list_is_empty_and_open_fails() {
    let fake = fake(16_000, 2);
    fake.add_loopback("spk", "Fake Speakers");
    fake.set_loopback_unsupported();
    assert!(fake.list_loopback_devices().unwrap().is_empty());
    assert!(
        support::open_with::<Chunks>(&fake, loopback_config("spk"), support::timeouts()).is_err()
    );
}

#[test]
fn microphone_permission_does_not_block_loopback() {
    let fake = fake(16_000, 2);
    fake.add_loopback("spk", "Fake Speakers");
    fake.set_permission(Permission::Denied);
    // CoreAudio-like (denial is silent): the microphone itself is refused...
    assert_eq!(
        support::open_with::<Chunks>(&fake, RecorderConfig::default(), support::timeouts())
            .err()
            .expect("the microphone should be refused")
            .kind(),
        ErrorKind::PermissionDenied
    );
    // ...but the output opens and records.
    let recorder: Recorder<Chunks> = open(&fake, loopback_config("spk"));
    start(&recorder, Chunks::default());
    assert!(fake.push(&[0.25_f32; 320]));
    let stopped = support::stop_with_boundary(&recorder, &fake, &[0.25_f32; 2]).expect("stop");
    assert!(stopped.is_complete());
    assert!(stopped.sink.valid_frames() > 0);
}
