# Fork of handy-recorder 0.1.1

Copied from crates.io (sha256 29602f34…255a). Adds loopback capture
(recording a sound card's output) for Windows and macOS.
Design: ../../docs/specs/system-audio-capture/design.md

Files changed from upstream are listed here as they change:

- src/device.rs, src/lib.rs, src/backend/cpal/mod.rs, src/backend/fake.rs
  (T2: `DeviceKind` and `InputDevice::kind`)
- src/backend/mod.rs (+ device.rs, lib.rs, cpal/mod.rs, fake.rs)
  (T3: `loopback:` ID scheme, `DeviceRef`, loopback listing API)
- src/backend/cpal/mod.rs, Cargo.toml (+ both Cargo.lock)
  (T4: list outputs through cpal; `objc2-foundation` for the macOS version check)
