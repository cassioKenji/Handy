# Tasks: captura do áudio de saída do sistema (loopback)

Fonte: [design.md](design.md) · [spec.md](spec.md)

Este arquivo divide a implementação em tasks pequenas, em ordem. Cada task diz
exatamente quais arquivos mexer, o que escrever, como verificar e qual commit
fazer. **Siga a ordem.** Cada task depende das anteriores.

---

## Regras gerais (leia antes de começar)

1. **Uma task por vez.** Termine a verificação da task antes de começar a
   próxima. Se uma verificação falhar, conserte antes de seguir. Não pule
   para a próxima task "para depois voltar".
2. **Não mexa em arquivos que a task não lista.** Se achar que precisa, pare
   e explique o motivo para o humano antes.
3. **Ache o código pelo texto, não pelo número da linha.** As linhas citadas
   ("~631") são aproximadas. Procure o trecho citado com a busca do editor
   ou com `grep -n`.
4. **Nunca use `unwrap()` em código novo de produção** (regra do projeto). Nos
   testes pode.
5. **Comentários e doc comments em inglês**, no estilo do arquivo: frases
   curtas que explicam o porquê. Textos para o usuário só via i18n.
6. **Onde rodar:** o build precisa de Rust e Bun no **Windows ou no macOS** (o
   WSL deste repo não tem cargo). Os comandos abaixo partem da raiz do repo:
   - Rust do app: `cd src-tauri && cargo check`, `cargo test`, `cargo clippy`
   - Crate: `cd crates/handy-recorder && cargo test --features test-backend`
   - Frontend: `bun run lint`, `bun run build`
   - Formatação: `bun run format` antes de cada commit
   - Traduções: `bun run check:translations`
7. **`src/bindings.ts` é gerado.** Ele é regenerado quando o app roda em
   debug (`bun run tauri dev`). Se não der para rodar o app, edite à mão
   seguindo os modelos da T14. Nunca deixe o `bindings.ts` fora de sincronia
   com os comandos Rust.
8. **Commits:** prefixos convencionais (`feat:`, `refactor:`, `chore:`,
   `test:`), com a mensagem focada no porquê. Use as mensagens sugeridas em
   cada task. Termine cada mensagem com a linha de co-autoria que o humano
   pedir.
9. **Em caso de dúvida**, o design manda. Se o design e esta task
   discordarem, **esta task vale** (ela foi escrita depois e com o código
   real na mão) e deve ser anotada no fim do PR.

Glossário:
- **loopback**: gravar o que um device de **saída** está tocando.
- **crate**: `crates/handy-recorder`, a biblioteca de captura que vamos
  copiar para dentro do repo.
- **cpal**: a biblioteca de áudio de baixo nível (`handy-cpal` 0.18.2). Ela
  **não** é alterada em nenhuma task.

---

## Fase A: a crate `handy-recorder`

### T1: Copiar a `handy-recorder` para dentro do repo, sem nenhuma alteração

**Objetivo:** o app passa a compilar com uma cópia local da crate, idêntica à
publicada. Nenhum comportamento muda.

**Passos:**

1. Baixe a crate exata que o `Cargo.lock` usa:
   ```bash
   mkdir -p /tmp/hr && cd /tmp/hr
   curl -L -o handy-recorder-0.1.1.crate \
     https://crates.io/api/v1/crates/handy-recorder/0.1.1/download
   sha256sum handy-recorder-0.1.1.crate
   ```
   O hash **tem que ser**
   `29602f3465cb080cc437b08903ea7ef395c55a41120264a897acdd25ff9b255a`, o
   `checksum` de `handy-recorder` em `src-tauri/Cargo.lock`. Se for
   diferente, pare e avise.
2. Extraia: `tar xzf handy-recorder-0.1.1.crate`. Isso cria a pasta
   `handy-recorder-0.1.1/`.
3. Copie a pasta para o repo como `crates/handy-recorder/` (crie a pasta
   `crates/` na raiz do repo, ao lado de `src-tauri/`). No Windows, copie com
   o Explorer ou com `robocopy`.
4. Apague só `crates/handy-recorder/.cargo_vcs_info.json`, se existir. Ele é
   metadado do publish. **Mantenha** o `Cargo.toml`, o `Cargo.toml.orig`
   (referência) e todo o resto.
5. Em `src-tauri/Cargo.toml`, troque a linha
   ```toml
   handy-recorder = "0.1.1"
   ```
   por
   ```toml
   # Vendored fork: records sound card outputs (loopback). See
   # docs/specs/system-audio-capture/.
   handy-recorder = { path = "../crates/handy-recorder" }
   ```
6. Crie `crates/handy-recorder/FORK.md` com:
   ```markdown
   # Fork of handy-recorder 0.1.1

   Copied from crates.io (sha256 29602f34…255a). Adds loopback capture
   (recording a sound card's output) for Windows and macOS.
   Design: ../../docs/specs/system-audio-capture/design.md

   Files changed from upstream are listed here as they change:
   ```
7. Rode `cd src-tauri && cargo check`. O `Cargo.lock` muda só na entrada de
   `handy-recorder` (a linha `source` e o `checksum` somem). Isso é esperado.

**Verificação:**
- `cd src-tauri && cargo check` passa.
- `cd crates/handy-recorder && cargo test --features test-backend` passa (os
  testes da própria crate).
- `git diff --stat` mostra só: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
  e os arquivos novos em `crates/`.

**Commit:** `chore: vendor handy-recorder 0.1.1 unchanged`
(Explicar no corpo: base do fork, para que as mudanças seguintes fiquem
legíveis no diff.)

---

### T2: `DeviceKind` e o campo `kind` em `InputDevice`

**Objetivo:** todo `InputDevice` diz se é um microfone ou uma saída
(loopback). Por enquanto todos são `Microphone`.

**Arquivos:** `crates/handy-recorder/src/device.rs`, `src/lib.rs`,
`src/backend/cpal/mod.rs`, `src/backend/fake.rs`.

**Passos:**

1. Em `device.rs`, logo **acima** de `pub struct InputDevice`, adicione:
   ```rust
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
   ```
2. Em `pub struct InputDevice`, adicione como **último** campo:
   ```rust
       /// A microphone, or an output recorded by loopback.
       pub kind: DeviceKind,
   ```
3. Em `lib.rs`, na linha
   `pub use device::{InputDevice, Permission, list_input_devices, permission_status};`,
   acrescente `DeviceKind`:
   ```rust
   pub use device::{DeviceKind, InputDevice, Permission, list_input_devices, permission_status};
   ```
4. O compilador vai apontar todo lugar que monta um `InputDevice { … }`.
   Rode `cargo check --features test-backend` na crate e, em **cada** erro
   `missing field kind`, acrescente `kind: DeviceKind::Microphone,`. Os
   lugares conhecidos:
   - `backend/cpal/mod.rs`: em `enumerate()` (`out.push((InputDevice { … }`)
     e em `open_device()` (o `let info = InputDevice { … }` dentro de
     `unwrap_or_else`).
   - `backend/fake.rs`: em `FakeBackend::new`.
   - Talvez em `src/testing.rs`: procure `InputDevice {`.
   Importe `DeviceKind` onde precisar (`use crate::DeviceKind;` ou junto do
   `use crate::{InputDevice, …}` que já existe).

**Verificação:**
- `cd crates/handy-recorder && cargo test --features test-backend` passa.
- `cd src-tauri && cargo check` passa.
- `grep -rn "InputDevice {" crates/handy-recorder/src` mostra `kind:` em
  todos os resultados que montam o struct.

**Commit:** `feat(recorder): tag each input device with its kind`

Acrescente `src/device.rs, src/lib.rs, src/backend/cpal/mod.rs,
src/backend/fake.rs` à lista do `FORK.md`.

---

### T3: Esquema de id `loopback:`, métodos novos do `Backend` e API pública

**Objetivo:** a crate expõe `list_loopback_devices()` e
`loopback_supported()`. O backend sabe separar um id de loopback de um id de
mic. O cpal ainda devolve uma lista vazia (a T4 preenche). O fake backend já
suporta loopback, para os testes.

**Arquivos:** `src/backend/mod.rs`, `src/device.rs`, `src/lib.rs`,
`src/backend/cpal/mod.rs`, `src/backend/fake.rs`.

**Passos:**

1. Em `backend/mod.rs`, logo depois dos `use` do topo, adicione:
   ```rust
   /// Prefix of every loopback device ID. CoreAudio gives a device's input
   /// and output the same UID, so the direction has to live in the ID.
   pub(crate) const LOOPBACK_PREFIX: &str = "loopback:";

   /// A `RecorderConfig::device`, split by direction.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub(crate) enum DeviceRef<'a> {
       /// `None`: the system's default input.
       Default,
       /// A microphone ID.
       Input(&'a str),
       /// An output ID, with `LOOPBACK_PREFIX` removed.
       Loopback(&'a str),
   }

   impl<'a> DeviceRef<'a> {
       pub fn parse(id: Option<&'a str>) -> Self {
           match id {
               None => Self::Default,
               Some(id) => match id.strip_prefix(LOOPBACK_PREFIX) {
                   Some(rest) => Self::Loopback(rest),
                   None => Self::Input(id),
               },
           }
       }
   }
   ```
2. No mesmo arquivo, no `pub(crate) trait Backend`, adicione depois de
   `fn list_input_devices`:
   ```rust
       /// Outputs that can be recorded by loopback; empty where unsupported.
       fn list_loopback_devices(&self) -> Result<Vec<InputDevice>, BackendError>;

       /// Whether this OS can record an output at all.
       fn loopback_supported(&self) -> bool;
   ```
3. No fim de `backend/mod.rs`, adicione testes do parse:
   ```rust
   #[cfg(test)]
   mod device_ref_tests {
       use super::DeviceRef;

       #[test]
       fn parses_each_direction() {
           assert_eq!(DeviceRef::parse(None), DeviceRef::Default);
           assert_eq!(DeviceRef::parse(Some("wasapi:{a}")), DeviceRef::Input("wasapi:{a}"));
           assert_eq!(
               DeviceRef::parse(Some("loopback:wasapi:{a}")),
               DeviceRef::Loopback("wasapi:{a}")
           );
           // Only the first prefix is removed.
           assert_eq!(
               DeviceRef::parse(Some("loopback:loopback:x")),
               DeviceRef::Loopback("loopback:x")
           );
       }
   }
   ```
4. Em `backend/cpal/mod.rs`, dentro de `impl Backend for CpalBackend`,
   implemente provisoriamente (a T4 troca pela versão real):
   ```rust
       fn list_loopback_devices(&self) -> Result<Vec<InputDevice>, BackendError> {
           Ok(Vec::new())
       }

       fn loopback_supported(&self) -> bool {
           false
       }
   ```
5. Em `device.rs`, adicione depois de `pub fn list_input_devices`:
   ```rust
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
   ```
6. Em `lib.rs`, exporte as duas funções no `pub use device::{…}`:
   ```rust
   pub use device::{
       DeviceKind, InputDevice, Permission, list_input_devices, list_loopback_devices,
       loopback_supported, permission_status,
   };
   ```
7. **Fake backend** (`backend/fake.rs`):
   a. No `struct State`, adicione:
      ```rust
          /// Loopback devices this fake lists, with `LOOPBACK_PREFIX` IDs.
          loopback: Vec<InputDevice>,
          /// Behave like an OS that can't record its output (default: it can).
          loopback_unsupported: bool,
      ```
      (`State` tem `#[derive(Default)]`: `Vec` e `bool` já têm default, então
      não precisa mudar mais nada.)
   b. No `impl FakeBackend`, adicione:
      ```rust
          /// Adds a loopback device; `raw_id` gets `LOOPBACK_PREFIX`.
          pub fn add_loopback(&self, raw_id: &str, name: &str) {
              let device = InputDevice {
                  id: format!("{LOOPBACK_PREFIX}{raw_id}"),
                  name: name.into(),
                  occurrence: 0,
                  backend: "Fake".into(),
                  is_default: self.shared.state.lock().unwrap().loopback.is_empty(),
                  id_is_stable: true,
                  channels: Some(self.shared.format.channels),
                  is_monitor: false,
                  kind: DeviceKind::Loopback,
              };
              self.shared.state.lock().unwrap().loopback.push(device);
          }

          /// Behave like macOS before 14.6.
          pub fn set_loopback_unsupported(&self) {
              self.shared.state.lock().unwrap().loopback_unsupported = true;
          }
      ```
      Imports necessários no topo: `use super::LOOPBACK_PREFIX;` e
      `use crate::DeviceKind;` (ou junte aos `use` existentes).
   c. Troque `struct FakeOpenDevice(FakeBackend);` por:
      ```rust
      struct FakeOpenDevice {
          backend: FakeBackend,
          info: InputDevice,
      }
      ```
      Ajuste o `impl OpenDevice for FakeOpenDevice`: `info()` retorna
      `&self.info`; `format()` usa `self.backend.shared.format`; em `start()`,
      troque todo `self.0` por `self.backend`; e o retorno final passa a ser
      `Ok(Box::new(FakeStream(self.backend)))`.
   d. No `impl Backend for FakeBackend`:
      ```rust
          fn list_loopback_devices(&self) -> Result<Vec<InputDevice>, BackendError> {
              let state = self.shared.state.lock().unwrap();
              if state.loopback_unsupported {
                  return Ok(Vec::new());
              }
              Ok(state.loopback.clone())
          }

          fn loopback_supported(&self) -> bool {
              !self.shared.state.lock().unwrap().loopback_unsupported
          }
      ```
      E reescreva `open_device` assim:
      ```rust
          fn open_device(&self, id: Option<&str>) -> Result<Box<dyn OpenDevice>, BackendError> {
              if let Some(error) = self.shared.state.lock().unwrap().next_open_error.take() {
                  return Err(error);
              }
              let info = match DeviceRef::parse(id) {
                  DeviceRef::Default => self.shared.device.clone(),
                  DeviceRef::Input(id) if id == self.shared.device.id => self.shared.device.clone(),
                  DeviceRef::Input(id) => {
                      return Err(BackendError::new(
                          BackendErrorKind::DeviceNotAvailable,
                          format!("No input device with ID {id:?}"),
                      ));
                  }
                  DeviceRef::Loopback(raw) => {
                      if !self.loopback_supported() {
                          return Err(BackendError::new(
                              BackendErrorKind::UnsupportedConfig,
                              "loopback is not supported",
                          ));
                      }
                      let full = format!("{LOOPBACK_PREFIX}{raw}");
                      let state = self.shared.state.lock().unwrap();
                      match state.loopback.iter().find(|d| d.id == full) {
                          Some(device) => device.clone(),
                          None => {
                              return Err(BackendError::new(
                                  BackendErrorKind::DeviceNotAvailable,
                                  format!("No output device with ID {raw:?}"),
                              ));
                          }
                      }
                  }
              };
              Ok(Box::new(FakeOpenDevice {
                  backend: self.clone(),
                  info,
              }))
          }
      ```
      Importe `DeviceRef` (`use super::DeviceRef;`).

**Verificação:**
- `cd crates/handy-recorder && cargo test --features test-backend` passa,
  incluindo `parses_each_direction`.
- `cd src-tauri && cargo check` passa.

**Commit:** `feat(recorder): add the loopback device API and ID scheme`

Atualize o `FORK.md` com `src/backend/mod.rs`.

---

### T4: Listar as saídas no backend cpal

**Objetivo:** no Windows (e no macOS 14.6+), `list_loopback_devices()`
devolve as saídas de verdade.

**Arquivos:** `src/backend/cpal/mod.rs`, `crates/handy-recorder/Cargo.toml`.

**Passos:**

1. **Dependência para macOS.** Em `crates/handy-recorder/Cargo.toml`, depois
   do bloco `[target.'cfg(target_os = "macos")'.dependencies.objc2-av-foundation]`,
   adicione:
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies.objc2-foundation]
   version = "0.3"
   features = ["std", "NSProcessInfo"]
   default-features = false
   ```
   Use a mesma versão `0.3.x` que já está no `src-tauri/Cargo.lock` (procure
   `name = "objc2-foundation"`), para não duplicar a crate.
2. Em `backend/cpal/mod.rs`, atualize os imports:
   ```rust
   use super::{
       Backend, BackendError, BackendErrorKind, DataCallback, DeviceFormat, DeviceRef,
       ErrorCallback, InputSample, InputStream, LOOPBACK_PREFIX, OpenDevice, SampleFormat,
   };
   use crate::{DeviceKind, InputDevice, Permission};
   ```
3. Adicione, perto das outras funções livres (`is_monitor`,
   `channels_without_opening`):
   ```rust
   /// Which list `enumerate` builds.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   enum Direction {
       Input,
       Loopback,
   }

   /// Whether the OS can record an output (cached; reading it on macOS asks
   /// Foundation for the OS version).
   fn os_supports_loopback() -> bool {
       static SUPPORTED: OnceLock<bool> = OnceLock::new();
       *SUPPORTED.get_or_init(|| {
           #[cfg(target_os = "windows")]
           {
               true
           }
           #[cfg(target_os = "macos")]
           {
               macos_at_least(14, 6)
           }
           #[cfg(not(any(target_os = "windows", target_os = "macos")))]
           {
               false
           }
       })
   }

   /// CPAL records an output through a process tap from macOS 14.6.
   #[cfg(target_os = "macos")]
   fn macos_at_least(major: isize, minor: isize) -> bool {
       use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
       let version = NSOperatingSystemVersion {
           majorVersion: major,
           minorVersion: minor,
           patchVersion: 0,
       };
       NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
   }

   /// Whether CPAL would record this output rather than something else.
   /// On macOS, CPAL opens a device that also has inputs as a microphone
   /// (`supports_input`), so only output-only devices can be recorded.
   fn loopback_capable(device: &cpal::Device) -> bool {
       #[cfg(target_os = "macos")]
       {
           device.default_input_config().is_err()
       }
       #[cfg(target_os = "windows")]
       {
           let _ = device;
           true
       }
       #[cfg(not(any(target_os = "windows", target_os = "macos")))]
       {
           let _ = device;
           false
       }
   }
   ```
   Se o compilador reclamar que `isOperatingSystemAtLeastVersion` é
   `unsafe`, envolva a chamada em `unsafe { … }` com um comentário
   `// SAFETY: reads the OS version; no preconditions.`. Se reclamar do tipo
   dos campos (`NSInteger`), use `major as _`.
4. **Reescreva `enumerate`** para receber a direção. Troque a função inteira
   por:
   ```rust
   /// Devices without a backend ID get one from name and occurrence ("USB Mic#1").
   fn enumerate(
       host: &cpal::Host,
       dir: Direction,
       with_channels: bool,
   ) -> Result<Vec<(InputDevice, cpal::Device)>, BackendError> {
       let default_id = match dir {
           Direction::Input => host.default_input_device(),
           Direction::Loopback => host.default_output_device(),
       }
       .and_then(|d| d.id().ok());
       let devices: Vec<cpal::Device> = match dir {
           Direction::Input => host.input_devices().map_err(map_error)?.collect(),
           Direction::Loopback => host.output_devices().map_err(map_error)?.collect(),
       };
       let backend = host.id().name().to_owned();
       let mut occurrences = HashMap::<String, u32>::new();
       let mut out = Vec::new();
       for device in devices {
           if device.id().is_ok_and(|id| is_null_pcm(&id)) {
               continue;
           }
           if dir == Direction::Loopback && !loopback_capable(&device) {
               continue;
           }
           let name = device_name(&device);
           let occurrence = occurrences.entry(name.clone()).or_default();
           let (raw_id, stable, is_default, monitor) = match device.id() {
               Ok(id) => (
                   id.to_string(),
                   id_is_stable(id.host()),
                   default_id.as_ref() == Some(&id),
                   is_monitor(&id),
               ),
               Err(_) => (format!("{name}#{occurrence}"), false, false, false),
           };
           let (id, kind, channels) = match dir {
               Direction::Input => (
                   raw_id,
                   DeviceKind::Microphone,
                   with_channels
                       .then(|| channels_without_opening(host.id(), &device))
                       .flatten(),
               ),
               Direction::Loopback => (
                   format!("{LOOPBACK_PREFIX}{raw_id}"),
                   DeviceKind::Loopback,
                   with_channels
                       .then(|| device.default_output_config().ok().map(|c| c.channels()))
                       .flatten(),
               ),
           };
           out.push((
               InputDevice {
                   id,
                   name,
                   occurrence: *occurrence,
                   backend: backend.clone(),
                   is_default,
                   id_is_stable: stable,
                   channels,
                   is_monitor: dir == Direction::Input && monitor,
                   kind,
               },
               device,
           ));
           *occurrence += 1;
       }
       Ok(out)
   }
   ```
5. Atualize **todas** as chamadas antigas de `enumerate(host, X)` para
   `enumerate(host, Direction::Input, X)`. Há três: em
   `list_input_devices` e duas em `open_device`. A T5 reescreve o
   `open_device`, mas ele tem que compilar agora.
6. Troque as duas implementações provisórias da T3 por:
   ```rust
       fn list_loopback_devices(&self) -> Result<Vec<InputDevice>, BackendError> {
           if !self.loopback_supported() {
               return Ok(Vec::new());
           }
           let host = self.host()?;
           Ok(enumerate(&host, Direction::Loopback, true)?
               .into_iter()
               .map(|(info, _)| info)
               .collect())
       }

       fn loopback_supported(&self) -> bool {
           os_supports_loopback()
       }
   ```
7. Estenda o teste de hardware `lists_and_resolves_real_devices` (no fim do
   arquivo, `#[ignore]`) com, **antes** da linha `let default = …`:
   ```rust
           for device in backend.list_loopback_devices().expect("list loopback") {
               assert_eq!(device.kind, DeviceKind::Loopback);
               assert!(device.id.starts_with(LOOPBACK_PREFIX));
               eprintln!(
                   "{} [loopback] {} id={} channels={:?}",
                   if device.is_default { "*" } else { " " },
                   device.name,
                   device.id,
                   device.channels
               );
           }
   ```
   Por enquanto ele não abre os devices de loopback. A T5 inclui isso.

**Verificação:**
- `cargo test --features test-backend` na crate passa.
- No Windows: `cargo test --features test-backend -- --ignored --nocapture lists_and_resolves_real_devices`
  imprime as linhas `[loopback]` com as saídas da máquina (alto-falantes,
  fones, HDMI). Cole a saída no PR.
- `cd src-tauri && cargo check` passa.

**Commit:** `feat(recorder): list sound card outputs for loopback capture`

---

### T5: Abrir um device de loopback

**Objetivo:** `Recorder::open` com `device: Some("loopback:…")` abre a saída
em modo loopback.

**Arquivo:** `src/backend/cpal/mod.rs`.

**Por que muda:** hoje o `open_device` chama `device.default_input_config()`.
Num device de saída, isso dá **erro no Windows** ("Device does not support
input"). Para loopback, a config certa é a `default_output_config()`. O cpal
liga o loopback sozinho quando o `build_input_stream` roda num device de
saída.

**Passos:**

1. **Extraia o final do `open_device`** numa função livre. Hoje, depois de
   resolver `(mut info, device)`, o `open_device` faz: busca a config,
   mapeia o `SampleFormat`, seta `info.channels` e monta o
   `CpalOpenDevice`. Mova esse trecho para:
   ```rust
   /// Builds the `OpenDevice` for a resolved device and the config to run it
   /// at (input config for a microphone, output config for loopback).
   fn finish_open(
       host: cpal::HostId,
       mut info: InputDevice,
       device: cpal::Device,
       config: cpal::SupportedStreamConfig,
   ) -> Result<Box<dyn OpenDevice>, BackendError> {
       let sample_format = match config.sample_format() {
           // ... COPIE o match inteiro que já existe, sem mudar ...
       };
       info.channels = Some(config.channels());
       Ok(Box::new(CpalOpenDevice {
           host,
           info,
           device_id: device.id().ok(),
           device,
           config,
           format: DeviceFormat {
               sample_rate: config.sample_rate(),
               channels: config.channels(),
               sample_format,
           },
       }))
   }
   ```
   (Mantenha os logs `debug!` de tempo onde fizer sentido: o
   `fetch_config` fica no chamador.)
2. **Reescreva o `open_device`** com `DeviceRef`:
   ```rust
       fn open_device(&self, id: Option<&str>) -> Result<Box<dyn OpenDevice>, BackendError> {
           let resolve_started = Instant::now();
           let host = self.host()?;
           let host = &*host;
           match DeviceRef::parse(id) {
               DeviceRef::Loopback(raw) => {
                   if !self.loopback_supported() {
                       return Err(BackendError::new(
                           BackendErrorKind::UnsupportedConfig,
                           "Recording a sound card's output needs Windows or macOS 14.6+",
                       ));
                   }
                   let full = format!("{LOOPBACK_PREFIX}{raw}");
                   let (info, device) = enumerate(host, Direction::Loopback, false)?
                       .into_iter()
                       .find(|(info, _)| info.id == full)
                       .ok_or_else(|| {
                           BackendError::new(
                               BackendErrorKind::DeviceNotAvailable,
                               format!("No output device with ID {raw:?}"),
                           )
                       })?;
                   log::debug!("resolve_device={:?}", resolve_started.elapsed());
                   let config_started = Instant::now();
                   // The output's own format: loopback captures the mix the
                   // device plays, and an output has no input config.
                   let config = device
                       .default_output_config()
                       .map_err(|e| map_error_during("Failed to fetch output config", e))?;
                   log::debug!("fetch_config={:?}", config_started.elapsed());
                   finish_open(host.id(), info, device, config)
               }
               DeviceRef::Input(id) => {
                   // ... o ramo `Some(id) => …` atual, com
                   //     enumerate(host, Direction::Input, false) ...
                   // depois: default_input_config() e finish_open(...)
               }
               DeviceRef::Default => {
                   // ... o ramo `None => …` atual, sem mudanças ...
                   // depois: default_input_config() e finish_open(...)
               }
           }
       }
   ```
   Os ramos `Input` e `Default` têm que ficar **com o mesmo comportamento**
   de antes (os mesmos logs, a mesma mensagem de erro). Só muda a
   organização.
3. No teste de hardware (`#[ignore]`), dentro do laço de loopback que a T4
   criou, depois do `eprintln!`, abra o device:
   ```rust
               let open = backend.open_device(Some(&device.id)).expect("open loopback");
               assert_eq!(open.info().kind, DeviceKind::Loopback);
               eprintln!("    format={:?}", open.format());
   ```

**Verificação:**
- `cargo test --features test-backend` na crate passa (os testes de mic não
  podem quebrar).
- No Windows: o teste `--ignored` imprime um `format=` para cada saída.
- `cd src-tauri && cargo check` passa.

**Commit:** `feat(recorder): open sound card outputs in loopback mode`

---

### T6: Keepalive silencioso no Windows

**Objetivo:** gravar uma saída **sem nada tocando** não derruba o recorder.

**Por quê:** o WASAPI loopback só entrega pacotes enquanto algum app toca
áudio naquela saída. Sem pacotes, o watchdog da crate falha com `NoAudio`
(10 s sem o primeiro bloco) ou `Stalled` (5 s sem blocos). Um stream de
saída tocando zeros no mesmo device mantém o loopback mandando pacotes de
silêncio.

**Arquivo:** `src/backend/cpal/mod.rs`.

**Passos:**

1. Ache `fn silent_output(id: &cpal::DeviceId)`. Troque o atributo
   `#[cfg(target_os = "macos")]` dele por:
   ```rust
   #[cfg(any(target_os = "macos", target_os = "windows"))]
   ```
   Atualize o doc comment: "Starts a stream playing silence on the output
   device `id`: holds a headset (macOS) or keeps a loopback capture
   delivering (Windows)."
2. No `struct CpalStream`, adicione um campo **depois** de `_stream` (a ordem
   importa: no drop, os campos caem na ordem em que foram declarados, e a
   captura tem que parar antes do render):
   ```rust
       /// Silent output on the recorded device, so a WASAPI loopback capture
       /// keeps delivering when nothing plays (it sends no packets otherwise,
       /// and the watchdog would fail the recorder).
       #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
       keepalive: Option<cpal::Stream>,
   ```
   Garanta que `_stream` continua sendo o **primeiro** campo.
3. Em `impl OpenDevice for CpalOpenDevice`, no método `start`, **logo antes**
   de `let build_started = Instant::now();`, adicione:
   ```rust
           #[cfg(target_os = "windows")]
           let keepalive = if self.info.kind == DeviceKind::Loopback {
               match self.device_id.as_ref().map(silent_output) {
                   Some(Ok(stream)) => {
                       log::debug!("loopback keepalive: playing silence on the recorded output");
                       Some(stream)
                   }
                   Some(Err(e)) => {
                       // Capture still works while something plays.
                       log::warn!("loopback keepalive failed, recording without it: {e}");
                       None
                   }
                   None => {
                       log::warn!("loopback keepalive skipped: the output has no ID");
                       None
                   }
               }
           } else {
               None
           };
           #[cfg(not(target_os = "windows"))]
           let keepalive: Option<cpal::Stream> = None;
   ```
4. No `Ok(Box::new(CpalStream { … }))` do fim de `start`, acrescente
   `keepalive,`.

**Verificação:**
- `cargo test --features test-backend` na crate passa.
- `cd src-tauri && cargo check` passa no Windows **e** compila no macOS (se
  houver um Mac à mão; se não, revise que todo uso de `keepalive` e de
  `silent_output` está sob o `cfg` certo).
- O teste manual vem na T18 (critério 2).

**Commit:** `feat(recorder): keep a Windows loopback capture alive in silence`

---

### T7: Ignorar a permissão de microfone no loopback

**Objetivo:** com o microfone negado nas configurações de privacidade, a
gravação de uma saída continua funcionando e não é rotulada como "permissão
negada".

**Arquivo:** `src/capture/engine.rs`.

Há **três** lugares. Em cada um, acrescente a condição "e o device não é
loopback". Importe `DeviceKind` no topo (`use crate::DeviceKind;` ou junto
dos `use crate::{…}`).

1. **`open_stream`**: ache
   ```rust
       // See `denial_is_silent`.
       if backend.denial_is_silent() && backend.permission_status() == Permission::Denied {
   ```
   e troque por:
   ```rust
       // See `denial_is_silent`. Microphone permission says nothing about
       // recording an output.
       if info.kind != DeviceKind::Loopback
           && backend.denial_is_silent()
           && backend.permission_status() == Permission::Denied
       {
   ```
2. **`Engine::start`**: ache
   ```rust
           // Revoked while open; see `denial_is_silent`.
           if self.backend.denial_is_silent() && self.backend.permission_status() == Permission::Denied
   ```
   e troque por:
   ```rust
           // Revoked while open; see `denial_is_silent`. Not for loopback.
           let is_loopback = self
               .shared
               .device
               .get()
               .is_some_and(|d| d.kind == DeviceKind::Loopback);
           if !is_loopback
               && self.backend.denial_is_silent()
               && self.backend.permission_status() == Permission::Denied
   ```
   (Mantenha o resto do `if` igual. Se `self.shared.device` tiver outro
   nome/tipo, veja como o `open_stream` faz `shared.device.set(...)` e use o
   getter correspondente.)
3. **`runtime_error`**: ache
   ```rust
       let kind = if kind == ErrorKind::DeviceLost
           && !backend.denial_is_silent()
           && backend.permission_status() == Permission::Denied
   ```
   e acrescente uma condição:
   ```rust
       let is_loopback = shared
           .device
           .get()
           .is_some_and(|d| d.kind == DeviceKind::Loopback);
       let kind = if kind == ErrorKind::DeviceLost
           && !is_loopback
           && !backend.denial_is_silent()
           && backend.permission_status() == Permission::Denied
   ```

**Verificação:** `cargo test --features test-backend` na crate passa. A T8
testa este comportamento.

**Commit:** `fix(recorder): don't apply microphone permission to loopback capture`

Atualize o `FORK.md` com `src/capture/engine.rs`.

---

### T8: Testes de loopback na crate

**Objetivo:** cobrir com o fake backend tudo o que as T3, T5 e T7 fizeram.

**Arquivos:** novo `src/tests/loopback.rs`, `src/tests/mod.rs`.

**Passos:**

1. Em `src/tests/mod.rs`, adicione `mod loopback;` (em ordem alfabética,
   entre `lifecycle` e `support`).
2. Crie `src/tests/loopback.rs`. Use `lifecycle.rs` como modelo de estilo e
   de helpers (`fake`, `open`, `open_with`, `start`, `push_idle`, `Chunks`).
   Escreva estes testes:

   ```rust
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
       assert!(support::open_with::<Chunks>(&fake, loopback_config("spk"), support::timeouts()).is_err());
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
       let stopped = recorder.stop().expect("stop");
       assert!(stopped.is_complete());
   }
   ```
   Se algum helper tiver outro nome ou outra assinatura em `support.rs`,
   **adapte o teste ao helper** e não mude o helper. Se o
   `fake.list_input_devices()` não compilar, falta o `use crate::backend::Backend;`.
   Se `open_with` não for `pub` em `support.rs`, ele é. Confira e use o
   caminho `super::support::open_with`.

**Verificação:** `cd crates/handy-recorder && cargo test --features test-backend`
passa, incluindo os 5 testes novos. Rode também `cargo clippy --features test-backend`
e corrija os avisos **nos arquivos que você tocou**.

**Commit:** `test(recorder): cover loopback listing, opening and permissions`

---

## Fase B: o backend do Handy (`src-tauri`)

### T9: Settings `input_source` e `selected_loopback_device`

**Arquivo:** `src-tauri/src/settings.rs`.

**Passos:**

1. Logo **acima** de `pub enum OverlayPosition` (e dos atributos `#[derive]`
   dele), adicione:
   ```rust
   /// Where dictation audio comes from.
   #[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
   #[serde(rename_all = "snake_case")]
   pub enum InputSource {
       /// A microphone (`selected_microphone`).
       #[default]
       Microphone,
       /// What a sound card is playing (`selected_loopback_device`).
       SystemOutput,
   }
   ```
2. Em `pub struct AppSettings`, **logo depois** de
   `pub selected_output_device: Option<String>,`, adicione:
   ```rust
       /// Record a microphone, or what a sound card is playing.
       #[serde(default)]
       pub input_source: InputSource,
       /// Name of the output to record; `None` follows the system's default
       /// output.
       #[serde(default)]
       pub selected_loopback_device: Option<String>,
   ```
3. Na função que monta os defaults (procure `selected_output_device: None,`
   dentro de `AppSettings { … }`, ~linha 1031), adicione logo depois:
   ```rust
           input_source: InputSource::Microphone,
           selected_loopback_device: None,
   ```
4. Nos testes, há um JSON de settings antigo (procure
   `"selected_output_device": null,` ~linha 1615). **Não** acrescente os
   campos novos nele: ele prova que um settings antigo, sem eles, ainda
   carrega. Adicione no `mod tests` do mesmo arquivo:
   ```rust
   #[test]
   fn input_source_defaults_to_microphone() {
       let settings: AppSettings = serde_json::from_value(serde_json::json!({})).unwrap();
       assert_eq!(settings.input_source, InputSource::Microphone);
       assert_eq!(settings.selected_loopback_device, None);
   }

   #[test]
   fn input_source_uses_snake_case() {
       assert_eq!(
           serde_json::to_value(InputSource::SystemOutput).unwrap(),
           serde_json::json!("system_output")
       );
   }
   ```
   Se `AppSettings` não desserializar de `{}` (algum campo sem
   `#[serde(default)]`), troque o primeiro teste por um que parta do JSON
   antigo já existente no arquivo e cheque os dois campos. Veja como o teste
   desse JSON faz o parse e copie o padrão.

**Verificação:** `cd src-tauri && cargo test settings` passa.

**Commit:** `feat: add the input source and loopback device settings`

---

### T10: Resolução do device: `DesiredDevice`, `pick_device`, `CaptureSourceError`

**Objetivo:** o manager decide **qual device abrir** para as duas fontes, com
uma função pura testável.

**Arquivo:** `src-tauri/src/managers/audio.rs`.

**Passos:**

1. **Imports.** No topo, troque
   `use crate::settings::{get_settings, write_settings, AppSettings, VadBackend};`
   por
   ```rust
   use crate::settings::{get_settings, write_settings, AppSettings, InputSource, VadBackend};
   ```
2. **Erro do Handy.** Logo **depois** de `fn recording_notice_code` (fim do
   bloco), adicione:
   ```rust
   /// A recording source Handy can't open, decided before the recorder is
   /// asked. `handy_recorder::Error` can't be built outside the crate.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum CaptureSourceError {
       /// No output to record: none listed, or listing failed (the detail).
       NoOutputDevice(String),
       /// This OS can't record its output (macOS before 14.6).
       Unsupported,
   }

   impl std::fmt::Display for CaptureSourceError {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           match self {
               Self::NoOutputDevice(detail) if detail.is_empty() => {
                   f.write_str("no output device to record")
               }
               Self::NoOutputDevice(detail) => write!(f, "no output device to record: {detail}"),
               Self::Unsupported => f.write_str("this OS can't record its sound output"),
           }
       }
   }

   impl std::error::Error for CaptureSourceError {}
   ```
3. **Troque os tipos.** Substitua o bloco inteiro de
   `enum DesiredMicrophone { … }` **e** de `struct MicrophoneResolution { … }`
   (com os doc comments) por:
   ```rust
   /// The device the settings ask for now. Clamshell and regular selections
   /// stay distinct so losing a clamshell-only device does not erase the
   /// user's normal microphone preference.
   #[derive(Debug, Clone, PartialEq, Eq)]
   enum DesiredDevice {
       DefaultMicrophone,
       Microphone(String),
       Clamshell(String),
       DefaultLoopback,
       Loopback(String),
   }

   impl DesiredDevice {
       fn source(&self) -> InputSource {
           match self {
               Self::DefaultLoopback | Self::Loopback(_) => InputSource::SystemOutput,
               _ => InputSource::Microphone,
           }
       }

       /// The cache key for a named device; defaults are never cached (the
       /// default changes under us).
       fn cache_key(&self) -> Option<CacheKey> {
           match self {
               Self::Microphone(name) | Self::Clamshell(name) => Some(CacheKey {
                   source: InputSource::Microphone,
                   name: name.clone(),
               }),
               Self::Loopback(name) => Some(CacheKey {
                   source: InputSource::SystemOutput,
                   name: name.clone(),
               }),
               Self::DefaultMicrophone | Self::DefaultLoopback => None,
           }
       }
   }

   /// A microphone and an output can share a name ("Realtek Audio").
   #[derive(Debug, Clone, PartialEq, Eq)]
   struct CacheKey {
       source: InputSource,
       name: String,
   }

   /// What `pick_device` needs from a listed device. `InputDevice` is
   /// non-exhaustive, so tests can't build one.
   #[derive(Debug, Clone, PartialEq, Eq)]
   struct DeviceEntry {
       id: String,
       name: String,
       is_default: bool,
   }

   impl From<handy_recorder::InputDevice> for DeviceEntry {
       fn from(d: handy_recorder::InputDevice) -> Self {
           Self {
               id: d.id,
               name: d.name,
               is_default: d.is_default,
           }
       }
   }

   /// The persisted preference resolved to a recorder device ID.
   /// `device: None` opens the system's default microphone (never used for
   /// loopback).
   #[derive(Debug, Clone, PartialEq, Eq)]
   struct DeviceResolution {
       device: Option<String>,
       source: InputSource,
       /// The ID came from `cached_device`, so it may be stale.
       from_cache: bool,
       /// The user's named selection that enumeration confirmed is missing;
       /// the caller resets that setting once the fallback opens.
       unavailable: Option<(InputSource, String)>,
   }
   ```
4. **Função pura.** Logo depois desses tipos, adicione:
   ```rust
   /// Picks the device for `desired` from a listing of its kind (microphones,
   /// or loopback outputs). `devices: None` means listing failed. Pure, so it
   /// can be tested without hardware.
   fn pick_device(
       desired: &DesiredDevice,
       devices: Option<&[DeviceEntry]>,
   ) -> Result<DeviceResolution, CaptureSourceError> {
       let resolved = |device: Option<String>, unavailable| DeviceResolution {
           device,
           source: desired.source(),
           from_cache: false,
           unavailable,
       };
       match desired {
           DesiredDevice::DefaultMicrophone => Ok(resolved(None, None)),
           DesiredDevice::Microphone(name) | DesiredDevice::Clamshell(name) => {
               let Some(devices) = devices else {
                   // A listing error may be transient: keep the preference.
                   return Ok(resolved(None, None));
               };
               match devices.iter().find(|d| d.name == *name) {
                   Some(d) => Ok(resolved(Some(d.id.clone()), None)),
                   None => {
                       let unavailable = matches!(desired, DesiredDevice::Microphone(_))
                           .then(|| (InputSource::Microphone, name.clone()));
                       Ok(resolved(None, unavailable))
                   }
               }
           }
           DesiredDevice::DefaultLoopback | DesiredDevice::Loopback(_) => {
               let devices = devices.ok_or_else(|| {
                   CaptureSourceError::NoOutputDevice("listing outputs failed".into())
               })?;
               if let DesiredDevice::Loopback(name) = desired {
                   if let Some(d) = devices.iter().find(|d| d.name == *name) {
                       return Ok(resolved(Some(d.id.clone()), None));
                   }
               }
               let fallback = devices
                   .iter()
                   .find(|d| d.is_default)
                   .or_else(|| devices.first())
                   .ok_or_else(|| CaptureSourceError::NoOutputDevice(String::new()))?;
               let unavailable = match desired {
                   DesiredDevice::Loopback(name) => {
                       Some((InputSource::SystemOutput, name.clone()))
                   }
                   _ => None,
               };
               Ok(resolved(Some(fallback.id.clone()), unavailable))
           }
       }
   }
   ```
5. **Campo do cache.** No `struct AudioRecordingManager`, troque
   `cached_device: Arc<Mutex<Option<(String, String)>>>,` por
   `cached_device: Arc<Mutex<Option<(CacheKey, String)>>>,`. Atualize o doc
   comment: "Keyed by source and name, so a microphone ID is never reused for
   an output with the same name". O `Arc::new(Mutex::new(None))` no `new()`
   não muda.
6. **`desired_microphone` vira `desired_device`.** Renomeie o método e troque
   o retorno para `DesiredDevice`. Coloque **no começo** do corpo:
   ```rust
           if settings.input_source == InputSource::SystemOutput {
               return match &settings.selected_loopback_device {
                   Some(name) => DesiredDevice::Loopback(name.clone()),
                   None => DesiredDevice::DefaultLoopback,
               };
           }
   ```
   No resto do corpo, troque `DesiredMicrophone::Clamshell` por
   `DesiredDevice::Clamshell`, `DesiredMicrophone::Selected` por
   `DesiredDevice::Microphone` e `DesiredMicrophone::Default` por
   `DesiredDevice::DefaultMicrophone`. Assim o clamshell nem é consultado no
   modo saída.
7. **Adicione um getter público** logo depois:
   ```rust
       /// The source the settings ask for now (for classifying a failed start
       /// before any stream opened).
       pub fn desired_source(&self) -> InputSource {
           get_settings(&self.app_handle).input_source
       }
   ```
8. **`resolve_microphone_device` vira `resolve_device`.** Substitua o método
   inteiro por:
   ```rust
       fn resolve_device(
           &self,
           settings: &AppSettings,
       ) -> Result<DeviceResolution, CaptureSourceError> {
           let desired = self.desired_device(settings);
           if desired == DesiredDevice::DefaultMicrophone {
               debug!("device resolve: no mic configured -> system default");
               return pick_device(&desired, None);
           }
           if desired.source() == InputSource::SystemOutput && !handy_recorder::loopback_supported()
           {
               return Err(CaptureSourceError::Unsupported);
           }

           // Cache hit: skip the enumeration. A stale ID (device unplugged)
           // fails at open, where the caller invalidates and retries fresh.
           let key = desired.cache_key();
           if let Some(key) = &key {
               if let Some((cached_key, id)) = self.cached_device.lock().unwrap().as_ref() {
                   if cached_key == key {
                       debug!("device resolve: cache hit for '{}'", key.name);
                       return Ok(DeviceResolution {
                           device: Some(id.clone()),
                           source: desired.source(),
                           from_cache: true,
                           unavailable: None,
                       });
                   }
               }
           }

           let enumerate_started = Instant::now();
           let listed = match desired.source() {
               InputSource::Microphone => handy_recorder::list_input_devices(),
               InputSource::SystemOutput => handy_recorder::list_loopback_devices(),
           };
           let devices: Option<Vec<DeviceEntry>> = match listed {
               Ok(devices) => Some(devices.into_iter().map(DeviceEntry::from).collect()),
               Err(e) => {
                   debug!("Failed to list devices: {}", e);
                   None
               }
           };
           let resolution = pick_device(&desired, devices.as_deref())?;
           debug!(
               "device resolve: enumerate={:?} (device={:?}, fallback={})",
               enumerate_started.elapsed(),
               resolution.device,
               resolution.unavailable.is_some()
           );
           // Cache only the user's own named device, never a fallback.
           if let (Some(key), Some(id), None) = (key, &resolution.device, &resolution.unavailable)
           {
               *self.cached_device.lock().unwrap() = Some((key, id.clone()));
           }
           Ok(resolution)
       }
   ```
   O `.lock().unwrap()` em mutex já existe no arquivo inteiro. Mantenha o
   padrão, porque esta regra do projeto vale para erros, não para locks.
9. **`persist_default_microphone_after_fallback` vira
   `persist_default_after_fallback`:**
   ```rust
       /// Keep persisted settings and the UI aligned with a successful runtime
       /// fallback. Re-read first so recovery cannot clear a device the user
       /// selected concurrently while the stream was being rebuilt.
       fn persist_default_after_fallback(&self, source: InputSource, unavailable_name: &str) {
           let mut settings = get_settings(&self.app_handle);
           let (slot, setting) = match source {
               InputSource::Microphone => (&mut settings.selected_microphone, "selected_microphone"),
               InputSource::SystemOutput => {
                   (&mut settings.selected_loopback_device, "selected_loopback_device")
               }
           };
           if slot.as_deref() != Some(unavailable_name) {
               return;
           }
           *slot = None;
           write_settings(&self.app_handle, settings);
           let _ = self.app_handle.emit(
               "settings-changed",
               serde_json::json!({ "setting": setting, "value": "Default" }),
           );
       }
   ```
   Se o borrow checker reclamar de `slot` emprestar `settings` até o
   `write_settings`, resolva com um bloco `{ … }` que só zera o campo e
   devolve um `bool`.
10. **Por enquanto**, em `start_microphone_stream`, ajuste só o necessário
    para compilar (a T11 termina):
    - `let mut resolution = self.resolve_microphone_device(&settings);`
      vira `let mut resolution = self.resolve_device(&settings)?;`
    - No retry (`resolution = self.resolve_microphone_device(&settings);`),
      use `resolution = self.resolve_device(&settings)?;`
    - O bloco `if let Some(unavailable_name) = resolution.unavailable_selected_microphone {`
      vira:
      ```rust
              if let Some((source, unavailable_name)) = resolution.unavailable.take() {
                  // Only after the fallback opened: a failed fallback must not
                  // erase the user's preference.
                  self.persist_default_after_fallback(source, &unavailable_name);
              }
      ```
11. **Testes** no `mod tests` do arquivo:
    ```rust
    fn entry(id: &str, name: &str, is_default: bool) -> DeviceEntry {
        DeviceEntry { id: id.into(), name: name.into(), is_default }
    }

    #[test]
    fn a_named_output_resolves_to_its_id() {
        let outs = [entry("loopback:a", "Speakers", true), entry("loopback:b", "Headphones", false)];
        let r = pick_device(&DesiredDevice::Loopback("Headphones".into()), Some(&outs)).unwrap();
        assert_eq!(r.device.as_deref(), Some("loopback:b"));
        assert_eq!(r.source, InputSource::SystemOutput);
        assert_eq!(r.unavailable, None);
    }

    #[test]
    fn the_default_output_is_resolved_explicitly() {
        let outs = [entry("loopback:a", "Speakers", false), entry("loopback:b", "HDMI", true)];
        let r = pick_device(&DesiredDevice::DefaultLoopback, Some(&outs)).unwrap();
        // Never None: None would open the default *microphone*.
        assert_eq!(r.device.as_deref(), Some("loopback:b"));
    }

    #[test]
    fn a_missing_output_falls_back_to_the_default_and_reports_it() {
        let outs = [entry("loopback:a", "Speakers", true)];
        let r = pick_device(&DesiredDevice::Loopback("Gone".into()), Some(&outs)).unwrap();
        assert_eq!(r.device.as_deref(), Some("loopback:a"));
        assert_eq!(r.unavailable, Some((InputSource::SystemOutput, "Gone".into())));
    }

    #[test]
    fn no_outputs_is_an_error_not_the_microphone() {
        assert_eq!(
            pick_device(&DesiredDevice::DefaultLoopback, Some(&[])),
            Err(CaptureSourceError::NoOutputDevice(String::new()))
        );
        assert!(matches!(
            pick_device(&DesiredDevice::Loopback("x".into()), None),
            Err(CaptureSourceError::NoOutputDevice(_))
        ));
    }

    #[test]
    fn microphone_resolution_is_unchanged() {
        let mics = [entry("m1", "USB Mic", false)];
        let found = pick_device(&DesiredDevice::Microphone("USB Mic".into()), Some(&mics)).unwrap();
        assert_eq!(found.device.as_deref(), Some("m1"));
        let missing = pick_device(&DesiredDevice::Microphone("Gone".into()), Some(&mics)).unwrap();
        assert_eq!(missing.device, None);
        assert_eq!(missing.unavailable, Some((InputSource::Microphone, "Gone".into())));
        // A clamshell device going missing doesn't reset the regular selection.
        let clam = pick_device(&DesiredDevice::Clamshell("Gone".into()), Some(&mics)).unwrap();
        assert_eq!(clam.unavailable, None);
        // A listing error keeps the preference.
        let failed = pick_device(&DesiredDevice::Microphone("USB Mic".into()), None).unwrap();
        assert_eq!((failed.device, failed.unavailable), (None, None));
    }

    #[test]
    fn a_microphone_and_an_output_with_the_same_name_have_different_cache_keys() {
        let mic = DesiredDevice::Microphone("Realtek Audio".into()).cache_key();
        let out = DesiredDevice::Loopback("Realtek Audio".into()).cache_key();
        assert_ne!(mic, out);
    }
    ```

**Verificação:**
- `cd src-tauri && cargo test managers::audio` passa (os testes novos e os
  antigos).
- `cargo check` passa.
- `grep -n "DesiredMicrophone\|MicrophoneResolution\|resolve_microphone_device\|persist_default_microphone_after_fallback" src-tauri/src -r`
  não retorna **nada**.

**Commit:** `feat: resolve a sound card output as the recording source`

---

### T11: Abrir o stream com a fonte certa e lembrar o que está aberto

**Objetivo:** em loopback, ignorar o canal e o modo AirPods. Guardar qual
fonte está aberta e qual foi a da última gravação.

**Arquivo:** `src-tauri/src/managers/audio.rs`.

**Passos:**

1. **Codificação atômica.** Logo depois de `const EARSHOT_VAD_THRESHOLD`,
   adicione:
   ```rust
   /// `InputSource` in an atomic, so hot paths read it without a lock.
   fn source_to_u8(source: InputSource) -> u8 {
       match source {
           InputSource::Microphone => 0,
           InputSource::SystemOutput => 1,
       }
   }

   fn source_from_u8(value: u8) -> InputSource {
       if value == 1 {
           InputSource::SystemOutput
       } else {
           InputSource::Microphone
       }
   }
   ```
   E no `use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};`,
   acrescente `AtomicU8`.
2. **`OpenMicrophone`** ganha um campo:
   ```rust
       /// What this recorder records: a microphone, or a sound card output.
       source: InputSource,
   ```
   Em `fn open_microphone`, no `Ok(OpenMicrophone { recorder, failed })`,
   acrescente `source: InputSource::Microphone`. Quem abre corrige o valor
   logo depois (passo 4).
3. **Manager:** no `struct AudioRecordingManager`, depois de
   `last_input_silent`, adicione:
   ```rust
       /// The source of the open (or last opened) stream. Mute, feedback
       /// sounds and notices follow what is actually recording, not the
       /// setting, which can change mid-recording.
       open_source: Arc<AtomicU8>,
       /// The source of the last finished recording, read with
       /// `last_input_silent`.
       last_input_source: Arc<AtomicU8>,
   ```
   E no `new()`: `open_source: Arc::new(AtomicU8::new(0)),` e
   `last_input_source: Arc::new(AtomicU8::new(0)),`.
4. **`start_microphone_stream`**: troque
   ```rust
           let channel = *self.selected_channel.lock().unwrap();
           let take_headset = self.airpods_mode.load(Ordering::Acquire);
   ```
   por
   ```rust
           // An output is mixed to mono, and AirPods mode is about microphones.
           let (channel, take_headset) = match resolution.source {
               InputSource::SystemOutput => (None, false),
               InputSource::Microphone => (
                   *self.selected_channel.lock().unwrap(),
                   self.airpods_mode.load(Ordering::Acquire),
               ),
           };
   ```
   O `?` no fim do `match open_microphone_on_channel(…) { … }?;` produz
   `microphone`. Troque `let microphone = match` por `let mut microphone = match`
   e, logo depois do `?;` (antes do `debug!("mic stream breakdown…`),
   adicione:
   ```rust
           microphone.source = resolution.source;
           self.open_source
               .store(source_to_u8(resolution.source), Ordering::Release);
   ```
   **Atenção:** no retry por cache, `resolution` é reatribuído dentro do
   `match`. Os valores de `channel`/`take_headset` calculados antes continuam
   válidos, porque a fonte não muda num retry (as mesmas settings).
5. **`collect_recording`**: nos dois lugares onde já existe
   `self.last_input_silent.store(…)` (no ramo `Ok(stopped)` e no ramo
   `Err(e)`), adicione logo depois:
   ```rust
                   self.last_input_source
                       .store(source_to_u8(microphone.source), Ordering::Release);
   ```
   (O parâmetro da função se chama `microphone: &OpenMicrophone`. Confira o
   nome.)
6. **Getters públicos**, perto de `pub fn last_input_silent`:
   ```rust
       /// The open (or last opened) stream records a sound card output.
       pub fn capturing_system_output(&self) -> bool {
           source_from_u8(self.open_source.load(Ordering::Acquire)) == InputSource::SystemOutput
       }

       /// The last finished recording came from a sound card output.
       pub fn last_input_was_system_output(&self) -> bool {
           source_from_u8(self.last_input_source.load(Ordering::Acquire))
               == InputSource::SystemOutput
       }
   ```

**Verificação:** `cd src-tauri && cargo test managers::audio` e
`cargo clippy` passam, sem avisos novos neste arquivo.

**Commit:** `feat: track whether the open stream records a sound card output`

---

### T12: Mute e sons de aviso no modo saída

**Arquivos:** `src-tauri/src/managers/audio.rs`, `src-tauri/src/actions.rs`.

**Passos:**

1. Em `apply_mute()`, troque
   ```rust
           if !settings.mute_while_recording {
               return;
           }
   ```
   por
   ```rust
           // Muting the system would silence the very output being recorded.
           if !settings.mute_while_recording || self.capturing_system_output() {
               return;
           }
   ```
   **Não** mexa em `remove_mute()`.
2. Em `actions.rs`, ache o som de **início**:
   ```rust
                       if rm_clone.is_recording_readiness_current(generation) {
                           play_feedback_sound_blocking(&app_clone, SoundType::Start);
                       }
   ```
   e troque a condição por:
   ```rust
                       // A sound played while recording an output would be
                       // transcribed.
                       if rm_clone.is_recording_readiness_current(generation)
                           && !rm_clone.capturing_system_output()
                       {
                           play_feedback_sound_blocking(&app_clone, SoundType::Start);
                       }
   ```
3. Ache o som de **fim**:
   ```rust
           // Play audio feedback for recording stop
           play_feedback_sound(app, SoundType::Stop);
   ```
   e troque por:
   ```rust
           // Play audio feedback for recording stop (not while recording an
           // output: the recording may still catch it).
           if !rm.capturing_system_output() {
               play_feedback_sound(app, SoundType::Stop);
           }
   ```
   (Confira que a variável do manager nesse ponto se chama `rm`. Ela é usada
   na linha de cima, `rm.remove_mute();`.)
4. **Não** mexa em `audio_feedback.rs` nem em `play_test_sound`.

**Verificação:** `cd src-tauri && cargo check` e `cargo test` passam.

**Commit:** `feat: skip mute and feedback sounds while recording an output`

---

### T13: Os três notices novos

Siga **exatamente** o passo a passo de "Error notices" no `AGENTS.md`. São
três códigos: `SystemAudioSilent` (Warning), `NoOutputDevice` (Error) e
`SystemAudioUnsupported` (Error).

**Arquivos:** `src-tauri/src/notices.rs`,
`src/i18n/locales/*/translation.json` (todos),
`src/lib/notices.ts`, `src-tauri/src/managers/audio.rs`,
`src-tauri/src/actions.rs`.

**Passos:**

1. **`notices.rs`:**
   - No `enum NoticeCode`, depois de `MicPermissionDenied,`, adicione:
     ```rust
         NoOutputDevice,
         SystemAudioSilent,
         SystemAudioUnsupported,
     ```
   - No `fn level`, adicione `| Self::NoOutputDevice | Self::SystemAudioUnsupported`
     à lista que retorna `NoticeLevel::Error`. Mude o braço do Warning para
     `Self::PostProcessFailed | Self::FilePostProcessFailed | Self::SystemAudioSilent => NoticeLevel::Warning,`.
     A gravação deu certo; só não tinha nada tocando.
   - No `fn tray_title`, adicione:
     ```rust
                 Self::NoOutputDevice => &strings.no_output_device_title,
                 Self::SystemAudioSilent => &strings.system_audio_silent_title,
                 Self::SystemAudioUnsupported => &strings.system_audio_unsupported_title,
     ```
     Esses campos são gerados pelo `build.rs` a partir das chaves
     `errors.*Title` do `en/translation.json` (passo 2). O código só compila
     depois do passo 2.
2. **Traduções.** Em `src/i18n/locales/en/translation.json`, no objeto
   `"errors"` da **raiz** (o que contém `"noInputDeviceTitle"`), adicione
   depois de `"microphoneSilent": "…",`:
   ```json
       "noOutputDeviceTitle": "No Output Device",
       "noOutputDevice": "There is no speaker or headphone output to record. Connect one, or switch back to the microphone in Settings.",
       "systemAudioSilentTitle": "Nothing Was Playing",
       "systemAudioSilent": {
         "generic": "Handy recorded your sound output, but nothing was playing.",
         "macos": "Handy recorded your sound output, but nothing was playing. If something was, allow Handy in System Settings → Privacy & Security → System Audio Recording."
       },
       "systemAudioUnsupportedTitle": "System Audio Not Supported",
       "systemAudioUnsupported": "Recording what your computer plays needs Windows, or macOS 14.6 or later. Switch back to the microphone in Settings.",
   ```
   Os títulos cabem no overlay (≤ 215px a 12px): os três têm menos de 30
   caracteres.

   **Os outros 28 locales** precisam das mesmas chaves, senão
   `bun run check:translations` falha. Use o script da seção
   [Apêndice A](#apêndice-a-script-para-adicionar-chaves-a-todos-os-locales),
   com estes valores (`pt` traduzido, os outros em inglês, que a comunidade
   traduz depois):
   ```ts
   const EN = {
     "errors.noOutputDeviceTitle": "No Output Device",
     "errors.noOutputDevice": "There is no speaker or headphone output to record. Connect one, or switch back to the microphone in Settings.",
     "errors.systemAudioSilentTitle": "Nothing Was Playing",
     "errors.systemAudioSilent.generic": "Handy recorded your sound output, but nothing was playing.",
     "errors.systemAudioSilent.macos": "Handy recorded your sound output, but nothing was playing. If something was, allow Handy in System Settings → Privacy & Security → System Audio Recording.",
     "errors.systemAudioUnsupportedTitle": "System Audio Not Supported",
     "errors.systemAudioUnsupported": "Recording what your computer plays needs Windows, or macOS 14.6 or later. Switch back to the microphone in Settings.",
   };
   const PT = {
     "errors.noOutputDeviceTitle": "Nenhuma Saída de Áudio",
     "errors.noOutputDevice": "Não há alto-falante ou fone para gravar. Conecte um, ou volte para o microfone nas Configurações.",
     "errors.systemAudioSilentTitle": "Nada Estava Tocando",
     "errors.systemAudioSilent.generic": "O Handy gravou a saída de som, mas nada estava tocando.",
     "errors.systemAudioSilent.macos": "O Handy gravou a saída de som, mas nada estava tocando. Se algo estava, permita o Handy em Ajustes do Sistema → Privacidade e Segurança → Gravação de Áudio do Sistema.",
     "errors.systemAudioUnsupportedTitle": "Áudio do Sistema Indisponível",
     "errors.systemAudioUnsupported": "Gravar o que o computador toca exige Windows, ou macOS 14.6 ou mais novo. Volte para o microfone nas Configurações.",
   };
   ```
   Rode o script **também** para o `en`. Se as chaves já existirem, ele não
   as sobrescreve, e o resultado é o mesmo. Depois rode
   `bun run format:frontend`.
3. **`src/lib/notices.ts`**, no `NOTICE_DISPLAY`, depois de `MicrophoneSilent: {…},`:
   ```ts
     NoOutputDevice: {
       title: "errors.noOutputDeviceTitle",
       explanation: (t) => t("errors.noOutputDevice"),
     },
     SystemAudioSilent: {
       title: "errors.systemAudioSilentTitle",
       explanation: (t) =>
         t(`errors.systemAudioSilent.${platform()}`, {
           defaultValue: t("errors.systemAudioSilent.generic"),
         }),
     },
     SystemAudioUnsupported: {
       title: "errors.systemAudioUnsupportedTitle",
       explanation: (t) => t("errors.systemAudioUnsupported"),
     },
   ```
   (O TypeScript só aceita isso depois que o `bindings.ts` tiver os códigos
   novos. Ver a T14. Se for editar o `bindings.ts` à mão agora, acrescente
   `| "NoOutputDevice" | "SystemAudioSilent" | "SystemAudioUnsupported"` ao
   `export type NoticeCode = …`.)
4. **`recording_notice_code`** em `managers/audio.rs`: substitua a função por:
   ```rust
   /// The notice for a recording that couldn't start. `source` is what the
   /// settings asked for: no stream may have opened.
   pub fn recording_notice_code(error: &anyhow::Error, source: InputSource) -> NoticeCode {
       if let Some(error) = error.downcast_ref::<CaptureSourceError>() {
           return match error {
               CaptureSourceError::NoOutputDevice(_) => NoticeCode::NoOutputDevice,
               CaptureSourceError::Unsupported => NoticeCode::SystemAudioUnsupported,
           };
       }
       let kind = error
           .downcast_ref::<handy_recorder::Error>()
           .map(handy_recorder::Error::kind);
       match source {
           // Microphone permission says nothing about an output.
           InputSource::SystemOutput => match kind {
               Some(ErrorKind::DeviceUnavailable) => NoticeCode::NoOutputDevice,
               _ => NoticeCode::RecordingFailed,
           },
           InputSource::Microphone => match kind {
               Some(ErrorKind::PermissionDenied) => NoticeCode::MicPermissionDenied,
               // A missing selected microphone falls back to the default, so this
               // means there is no input device at all.
               Some(ErrorKind::DeviceUnavailable) => NoticeCode::NoMicrophone,
               _ => NoticeCode::RecordingFailed,
           },
       }
   }
   ```
   Nos testes existentes do arquivo, troque `recording_notice_code(&wrapped)`
   por `recording_notice_code(&wrapped, InputSource::Microphone)` e
   `recording_notice_code(&anyhow::anyhow!("Already recording"))` por
   `recording_notice_code(&anyhow::anyhow!("Already recording"), InputSource::Microphone)`.
   Adicione:
   ```rust
   #[test]
   fn capture_source_errors_map_to_their_notices() {
       let no_output = anyhow::Error::new(CaptureSourceError::NoOutputDevice(String::new()))
           .context("Failed to open recorder");
       assert_eq!(
           recording_notice_code(&no_output, InputSource::SystemOutput),
           NoticeCode::NoOutputDevice
       );
       let unsupported = anyhow::Error::new(CaptureSourceError::Unsupported);
       assert_eq!(
           recording_notice_code(&unsupported, InputSource::SystemOutput),
           NoticeCode::SystemAudioUnsupported
       );
   }

   #[test]
   fn other_errors_while_recording_an_output_are_generic() {
       let wrapped = anyhow::Error::new(recorder_error());
       assert_eq!(
           recording_notice_code(&wrapped, InputSource::SystemOutput),
           NoticeCode::RecordingFailed
       );
   }
   ```
5. **`actions.rs`**:
   - Na chamada `Notice::new(recording_notice_code(&err), NoticeSource::Dictation)`,
     passe a fonte: `recording_notice_code(&err, rm.desired_source())`. Ache
     o nome da variável do `AudioRecordingManager` nessa função (procure
     para cima por `AudioRecordingManager` ou `rm`). Se não houver nenhuma,
     use
     `app.state::<Arc<AudioRecordingManager>>().desired_source()`.
   - Mude `fn finish_without_text`:
     ```rust
     /// Ends a dictation that has nothing to type. Input that was only digital
     /// silence gets a notice, so it doesn't look like the model missed the
     /// speech (#1899): a muted microphone, or an output with nothing playing.
     fn finish_without_text(
         app: &AppHandle,
         input_silent: bool,
         from_system_output: bool,
         sample_count: usize,
         stop_time: Instant,
     ) {
         if input_silent {
             let code = if from_system_output {
                 warn!("The recorded output was silent; nothing was transcribed");
                 NoticeCode::SystemAudioSilent
             } else {
                 warn!("The microphone sent only digital silence; nothing was transcribed");
                 NoticeCode::MicrophoneSilent
             };
             notices::raise(app, Notice::new(code, NoticeSource::Dictation));
         } else {
             utils::hide_recording_overlay(app);
         }
         set_tray_state(app, TrayIconState::Idle);
         emit_transcription_complete(app, empty_outcome(input_silent), sample_count, stop_time);
     }
     ```
   - Onde se lê `let input_silent = rm.last_input_silent();`, adicione logo
     abaixo:
     ```rust
                     let from_system_output = rm.last_input_was_system_output();
     ```
   - Nas **duas** chamadas `finish_without_text(&ah, input_silent, …)`,
     acrescente `from_system_output` como terceiro argumento:
     `finish_without_text(&ah, input_silent, from_system_output, 0, stop_time);` e
     `finish_without_text(&ah, input_silent, from_system_output, sample_count, stop_time);`.
     Se o compilador reclamar de "moved value" por causa do `async move`,
     a variável é `bool` (Copy) e não deveria. Confira que ela foi declarada
     dentro do mesmo bloco.

**Verificação:**
- `bun run check:translations` passa.
- `cd src-tauri && cargo test` passa.
- `grep -rn "systemAudioSilentTitle" src/i18n/locales | wc -l` mostra
  **29**, um por locale.

**Commit:** `feat: report silent, missing and unsupported output recording`

---

### T14: Comandos Tauri e bindings

**Arquivos:** `src-tauri/src/commands/audio.rs`, `src-tauri/src/lib.rs`,
`src/bindings.ts`.

**Passos:**

1. Em `commands/audio.rs`, ajuste o import das settings:
   `use crate::settings::{get_settings, write_settings, InputSource};`.
2. Adicione, depois de `get_selected_microphone`:
   ```rust
   #[tauri::command]
   #[specta::specta]
   pub fn get_input_source(app: AppHandle) -> InputSource {
       get_settings(&app).input_source
   }

   #[tauri::command]
   #[specta::specta]
   pub async fn set_input_source(app: AppHandle, source: InputSource) -> Result<(), String> {
       let mut settings = get_settings(&app);
       settings.input_source = source;
       write_settings(&app, settings);

       // Reopening can block (CoreAudio, WASAPI): keep it off the main run loop.
       let rm = app.state::<Arc<AudioRecordingManager>>().inner().clone();
       tokio::task::spawn_blocking(move || rm.update_selected_device())
           .await
           .map_err(|e| format!("audio task join failed: {}", e))?
           .map_err(|e| format!("Failed to switch input source: {}", e))
   }

   #[tauri::command]
   #[specta::specta]
   pub fn is_loopback_supported() -> bool {
       handy_recorder::loopback_supported()
   }

   #[tauri::command]
   #[specta::specta]
   pub async fn get_available_loopback_devices() -> Result<Vec<AudioDevice>, String> {
       // cpal device enumeration can stall — run it off the webview/main run loop.
       tokio::task::spawn_blocking(|| {
           let devices = handy_recorder::list_loopback_devices()
               .map_err(|e| format!("Failed to list output devices: {}", e))?;

           let mut result = vec![AudioDevice {
               index: "default".to_string(),
               name: "Default".to_string(),
               is_default: true,
           }];
           result.extend(devices.into_iter().map(|d| AudioDevice {
               index: d.id,
               name: d.name,
               is_default: false, // The explicit default is handled separately
           }));
           Ok::<_, String>(result)
       })
       .await
       .map_err(|e| format!("audio task join failed: {}", e))?
   }

   #[tauri::command]
   #[specta::specta]
   pub async fn set_selected_loopback_device(
       app: AppHandle,
       device_name: String,
   ) -> Result<(), String> {
       let mut settings = get_settings(&app);
       settings.selected_loopback_device = if device_name == "default" {
           None
       } else {
           Some(device_name)
       };
       let recording_output = settings.input_source == InputSource::SystemOutput;
       write_settings(&app, settings);

       // The open stream only changes when it records an output.
       if !recording_output {
           return Ok(());
       }
       let rm = app.state::<Arc<AudioRecordingManager>>().inner().clone();
       tokio::task::spawn_blocking(move || rm.update_selected_device())
           .await
           .map_err(|e| format!("audio task join failed: {}", e))?
           .map_err(|e| format!("Failed to update selected output: {}", e))
   }

   #[tauri::command]
   #[specta::specta]
   pub fn get_selected_loopback_device(app: AppHandle) -> Result<String, String> {
       Ok(get_settings(&app)
           .selected_loopback_device
           .unwrap_or_else(|| "default".to_string()))
   }
   ```
3. Em `lib.rs`, dentro de `collect_commands![ … ]`, logo depois de
   `commands::audio::get_selected_microphone,`, adicione:
   ```rust
               commands::audio::get_input_source,
               commands::audio::set_input_source,
               commands::audio::is_loopback_supported,
               commands::audio::get_available_loopback_devices,
               commands::audio::set_selected_loopback_device,
               commands::audio::get_selected_loopback_device,
   ```
4. **Bindings.** Rode `bun run tauri dev` (debug). Ao iniciar, o app
   reescreve o `src/bindings.ts`. Feche o app e confira com `git diff src/bindings.ts`:
   - os 6 comandos novos (`getInputSource`, `setInputSource`,
     `isLoopbackSupported`, `getAvailableLoopbackDevices`,
     `setSelectedLoopbackDevice`, `getSelectedLoopbackDevice`);
   - `export type InputSource = "microphone" | "system_output"` (com os
     doc comments);
   - `input_source?: InputSource; selected_loopback_device?: string | null`
     dentro de `AppSettings`;
   - os 3 `NoticeCode` novos.

   **Se não for possível rodar o app**, edite à mão copiando os modelos que
   já existem no arquivo: `getSelectedMicrophone` para os `get_*` que
   retornam `Result`, `setSelectedMicrophone` para os `set_*`, e um comando
   que retorna valor simples (procure um `Promise<boolean>` sem `Result`)
   para `getInputSource` e `isLoopbackSupported`. Nos que não retornam
   `Result`, o corpo é só `return await TAURI_INVOKE("is_loopback_supported");`.
   O nome do argumento no `TAURI_INVOKE` é o nome Rust em camelCase:
   `{ source }` e `{ deviceName }`. Avise no PR que o arquivo foi editado à
   mão e precisa ser regenerado.

**Verificação:**
- `cd src-tauri && cargo check` e `bun run build` passam.
- No app em dev, abra o devtools (Ctrl+Shift+I) e rode
  `await window.__TAURI_INTERNALS__.invoke("get_available_loopback_devices")`.
  O resultado lista "Default" e as saídas.

**Commit:** `feat: expose input source and loopback device commands`

---

## Fase C: frontend

### T15: Store e hook

**Arquivos:** `src/stores/settingsStore.ts`, `src/hooks/useSettings.ts`.

**Passos (`settingsStore.ts`):**

1. Na `interface SettingsStore`, depois de `outputDevices: AudioDevice[];`:
   ```ts
     loopbackDevices: AudioDevice[];
     /** Whether this OS can record its sound output. */
     loopbackSupported: boolean;
   ```
   E depois de `refreshOutputDevices: () => Promise<void>;`:
   ```ts
     refreshLoopbackDevices: () => Promise<void>;
     loadLoopbackSupported: () => Promise<void>;
   ```
2. No objeto de updaters (onde está `selected_microphone: (value) =>`),
   adicione:
   ```ts
     input_source: (value) => commands.setInputSource(value as InputSource),
     selected_loopback_device: (value) =>
       commands.setSelectedLoopbackDevice(
         (value as string) === "Default" || value === null
           ? "default"
           : (value as string),
       ),
   ```
   E importe o tipo: acrescente `InputSource,` ao `import type { … } from "@/bindings";`.
3. No estado inicial (onde estão `audioDevices: [],` e `outputDevices: [],`):
   ```ts
       loopbackDevices: [],
       loopbackSupported: false,
   ```
4. Em `refreshSettings`, dentro de `normalizedSettings`, depois de
   `selected_output_device: …,`:
   ```ts
             selected_loopback_device:
               settings.selected_loopback_device ?? "Default",
             input_source: settings.input_source ?? "microphone",
   ```
5. Depois da ação `refreshOutputDevices: async () => { … },`, adicione
   (copiando a estrutura de `refreshAudioDevices`):
   ```ts
       // Load the outputs that can be recorded (loopback)
       refreshLoopbackDevices: async () => {
         try {
           const result = await commands.getAvailableLoopbackDevices();
           if (result.status === "ok") {
             const devicesWithDefault = [
               DEFAULT_AUDIO_DEVICE,
               ...result.data.filter(
                 (d) => d.name !== "Default" && d.name !== "default",
               ),
             ];
             set({ loopbackDevices: devicesWithDefault });
           } else {
             set({ loopbackDevices: [DEFAULT_AUDIO_DEVICE] });
           }
         } catch (error) {
           console.error("Failed to load loopback devices:", error);
           set({ loopbackDevices: [DEFAULT_AUDIO_DEVICE] });
         }
       },

       loadLoopbackSupported: async () => {
         try {
           set({ loopbackSupported: await commands.isLoopbackSupported() });
         } catch (error) {
           console.error("Failed to check loopback support:", error);
           set({ loopbackSupported: false });
         }
       },
   ```
   (Se o `bindings.ts` gerar `isLoopbackSupported` como `Result`, trate como
   em `refreshAudioDevices`: `result.status === "ok" ? result.data : false`.)
6. Em `initialize`, dentro do `await Promise.all([ … ])`, acrescente
   `get().refreshLoopbackDevices(),` e `get().loadLoopbackSupported(),`.
   Copie o formato dos itens que já estão lá.
7. No listener `settings-changed`, depois do `if` de `selected_microphone`:
   ```ts
           if (event.payload.setting === "selected_loopback_device") {
             get().refreshLoopbackDevices();
           }
   ```

**Passos (`useSettings.ts`):** copie como `audioDevices` e
`refreshAudioDevices` são expostos (na interface e no objeto retornado) e
exponha `loopbackDevices`, `loopbackSupported` e `refreshLoopbackDevices` da
mesma forma.

**Verificação:** `bun run lint` e `bun run build` passam.

**Commit:** `feat: load loopback devices and support in the settings store`

---

### T16: Componentes e a seção Sound

**Arquivos:** novos `src/components/settings/InputSourceSelector.tsx` e
`src/components/settings/SystemOutputSelector.tsx`;
`src/components/settings/general/GeneralSettings.tsx`;
`src/components/settings/index.ts`; todos os `translation.json`.

**Passos:**

1. **i18n primeiro.** Em `en/translation.json`, ache o objeto
   `settings.sound` (o que contém `"microphone": { "title": "Microphone", …}`)
   e adicione, **antes** de `"microphone"`:
   ```json
         "inputSource": {
           "title": "Input Source",
           "description": "Record from a microphone, or from what your computer is playing",
           "microphone": "Microphone",
           "systemOutput": "System Output",
           "unsupported": "Recording what your computer plays needs macOS 14.6 or later"
         },
         "systemOutput": {
           "title": "Output to Record",
           "description": "Which speakers or headphones to record",
           "placeholder": "Select output...",
           "loading": "Loading..."
         },
   ```
   Depois rode o script do [Apêndice A](#apêndice-a-script-para-adicionar-chaves-a-todos-os-locales)
   com:
   ```ts
   const EN = {
     "settings.sound.inputSource.title": "Input Source",
     "settings.sound.inputSource.description": "Record from a microphone, or from what your computer is playing",
     "settings.sound.inputSource.microphone": "Microphone",
     "settings.sound.inputSource.systemOutput": "System Output",
     "settings.sound.inputSource.unsupported": "Recording what your computer plays needs macOS 14.6 or later",
     "settings.sound.systemOutput.title": "Output to Record",
     "settings.sound.systemOutput.description": "Which speakers or headphones to record",
     "settings.sound.systemOutput.placeholder": "Select output...",
     "settings.sound.systemOutput.loading": "Loading...",
   };
   const PT = {
     "settings.sound.inputSource.title": "Fonte de Entrada",
     "settings.sound.inputSource.description": "Gravar de um microfone, ou do que o computador está tocando",
     "settings.sound.inputSource.microphone": "Microfone",
     "settings.sound.inputSource.systemOutput": "Saída do Sistema",
     "settings.sound.inputSource.unsupported": "Gravar o que o computador toca exige macOS 14.6 ou mais novo",
     "settings.sound.systemOutput.title": "Saída a Gravar",
     "settings.sound.systemOutput.description": "Quais alto-falantes ou fones gravar",
     "settings.sound.systemOutput.placeholder": "Selecione a saída...",
     "settings.sound.systemOutput.loading": "Carregando...",
   };
   ```
   Confira se o objeto `settings.sound` fica no mesmo caminho em todos os
   locales. O script cria os objetos que faltarem.
2. **`InputSourceSelector.tsx`**: use o `MicrophoneSelector.tsx` como modelo
   de imports, props e `React.memo`:
   ```tsx
   import React from "react";
   import { useTranslation } from "react-i18next";
   import type { InputSource } from "@/bindings";
   import { Dropdown } from "../ui/Dropdown";
   import { SettingContainer } from "../ui/SettingContainer";
   import { useSettings } from "../../hooks/useSettings";

   interface InputSourceSelectorProps {
     descriptionMode?: "inline" | "tooltip";
     grouped?: boolean;
   }

   /** Microphone, or what the computer is playing (loopback). */
   export const InputSourceSelector: React.FC<InputSourceSelectorProps> =
     React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
       const { t } = useTranslation();
       const { getSetting, updateSetting, isUpdating, isLoading, loopbackSupported } =
         useSettings();
       const source: InputSource = getSetting("input_source") ?? "microphone";

       const options = [
         { value: "microphone", label: t("settings.sound.inputSource.microphone") },
         { value: "system_output", label: t("settings.sound.inputSource.systemOutput") },
       ];

       return (
         <SettingContainer
           title={t("settings.sound.inputSource.title")}
           description={
             loopbackSupported
               ? t("settings.sound.inputSource.description")
               : t("settings.sound.inputSource.unsupported")
           }
           descriptionMode={descriptionMode}
           grouped={grouped}
         >
           <Dropdown
             options={options}
             selectedValue={source}
             onSelect={(value) => updateSetting("input_source", value as InputSource)}
             disabled={
               isLoading ||
               isUpdating("input_source") ||
               // Still let someone on an unsupported OS switch back.
               (!loopbackSupported && source === "microphone")
             }
           />
         </SettingContainer>
       );
     });
   ```
   **Confira as props reais do `Dropdown`** em `src/components/ui/Dropdown.tsx`
   e ajuste (por exemplo, se `placeholder` for obrigatório, passe
   `t("settings.sound.inputSource.title")`). Não mude o `Dropdown`.
3. **`SystemOutputSelector.tsx`**: copie o `MicrophoneSelector.tsx` inteiro
   e troque:
   - `MicrophoneDropdown` → `SystemOutputDropdown` (componente interno, não
     exportado, sem as props `placement`/`tone`/`onPicked`);
   - `MicrophoneSelector` → `SystemOutputSelector`;
   - `"selected_microphone"` → `"selected_loopback_device"` (em todos os
     lugares);
   - `audioDevices` → `loopbackDevices`, `refreshAudioDevices` →
     `refreshLoopbackDevices`;
   - as chaves `settings.sound.microphone.*` → `settings.sound.systemOutput.*`;
   - o comentário do topo: `/** The outputs that can be recorded, as a dropdown. */`.
4. **`index.ts`**, depois de `export { MicrophoneSelector } …`:
   ```ts
   export { InputSourceSelector } from "./InputSourceSelector";
   export { SystemOutputSelector } from "./SystemOutputSelector";
   ```
5. **`GeneralSettings.tsx`**: importe os dois componentes (no mesmo estilo
   dos imports `../MicrophoneSelector`). Antes do `return`, adicione
   `const inputSource = getSetting("input_source") ?? "microphone";`. Troque
   ```tsx
           <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
           <ChannelSelector descriptionMode="tooltip" grouped={true} />
           <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
   ```
   por
   ```tsx
           <InputSourceSelector descriptionMode="tooltip" grouped={true} />
           {inputSource === "system_output" ? (
             <SystemOutputSelector descriptionMode="tooltip" grouped={true} />
           ) : (
             <>
               <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
               <ChannelSelector descriptionMode="tooltip" grouped={true} />
               <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
             </>
           )}
   ```
   **Não** mexa em `AudioFeedback`, `OutputDeviceSelector` e `VolumeSlider`.
   **Não** mexa no onboarding (`TryIt.tsx`) nem no `DebugSettings`.

**Verificação:**
- `bun run lint` (sem string hardcoded em JSX), `bun run build` e
  `bun run check:translations` passam.
- No app em dev: Settings → General → Sound mostra "Input Source". Trocar
  para "System Output" esconde Microphone, Input Channel e Mute while
  recording, e mostra "Output to Record" com as saídas. Voltar para
  "Microphone" mostra o microfone selecionado antes.

**Commit:** `feat: add the input source and output selectors to Sound settings`

---

## Fase D: macOS e validação

### T17: Permissão de áudio do sistema no macOS

**Arquivo:** `src-tauri/Info.plist`.

**Passo:** dentro do `<dict>`, depois do par `NSMicrophoneUsageDescription`:
```xml
  <key>NSAudioCaptureUsageDescription</key>
  <string>Record what your Mac is playing so it can be transcribed locally</string>
```
Não mexa no `Entitlements.plist` nesta task (design §6 e spec R2: só se o
teste num build assinado falhar).

**Verificação:** `plutil -lint src-tauri/Info.plist` (no Mac) diz `OK`.

**Commit:** `feat(macos): ask for system audio recording permission`

---

### T18: Validação final e PR

1. **Rode tudo:**
   ```bash
   bun run format
   bun run lint
   bun run build
   bun run check:translations
   cd src-tauri && cargo clippy && cargo test && cd ..
   cd crates/handy-recorder && cargo clippy --features test-backend && cargo test --features test-backend && cd ../..
   ```
   Tudo verde. Avisos de clippy em arquivos que você **não** tocou podem
   ficar.
2. **Teste manual.** Siga a matriz da [spec §6](spec.md#6-critérios-de-aceite)
   e anote passou/falhou para cada critério:

   | # | Como testar |
   |---|---|
   | 1 | Windows, Saída do sistema + "Default". Toque um vídeo com fala no navegador e use o atalho por ~5 s. O texto é colado. |
   | 2 | Windows, nada tocando, segure o atalho por **20 s**. Não aparece erro de gravação; ao soltar aparece "Nothing Was Playing". |
   | 3 | Windows, escolha um fone USB em "Output to Record" e toque o vídeo **nos alto-falantes**: o texto sai vazio. Toque **no fone**: o texto sai. |
   | 4 | macOS 14.6+, alto-falantes internos, como o 1. No primeiro uso aparece o pedido de "System Audio Recording". |
   | 5 | macOS < 14.6 (ou VM): a opção aparece desabilitada, com a explicação. |
   | 6 | Ligue "Mute while recording" no modo microfone, troque para Saída do sistema e grave um vídeo: o som **não** é mutado e o texto sai. |
   | 7 | Com os sons de aviso ligados, grave a saída: os sons não tocam e o texto não traz o "plim". |
   | 8 | Volte para Microfone: o mic de antes está selecionado e funciona. |
   | 9 | Numa placa com mic e saída de mesmo nome (Realtek), selecione a saída e grave: vem o som da saída, não do mic. |
   | 10 | Ligue "Always-on microphone" (Advanced), troque de fonte várias vezes: cada troca vale na hora, sem reiniciar. |

   Para cada falha, abra o log em debug (`--debug` ou Ctrl+Shift+D) e
   procure `loopback`, `device resolve` e `keepalive`.
3. **PR.** Leia `.github/PULL_REQUEST_TEMPLATE.md` e siga **todas** as seções
   (AGENTS.md: obrigatório). Na "Human Written Description", deixe
   `TODO: (humano) descrever com suas palavras` e peça ao humano para
   preencher. Inclua no PR:
   - a saída do teste `--ignored` de hardware (T4/T5);
   - a tabela do teste manual preenchida;
   - qualquer ponto em que você se afastou do design (regra geral 9);
   - se o `bindings.ts` foi editado à mão (T14).

---

## Apêndice A: script para adicionar chaves a todos os locales

Salve **fora do repo** (ex.: `/tmp/add-locale-keys.ts`), cole os objetos
`EN` e `PT` da task e rode da raiz do repo com `bun /tmp/add-locale-keys.ts`.
O script **não sobrescreve** chaves que já existem e cria os objetos
intermediários que faltarem.

```ts
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Cole aqui os objetos da task: chave pontuada -> texto.
const EN: Record<string, string> = {
  // "errors.noOutputDeviceTitle": "No Output Device",
};
const PT: Record<string, string> = {
  // "errors.noOutputDeviceTitle": "Nenhuma Saída de Áudio",
};

const LOCALES = join(process.cwd(), "src", "i18n", "locales");

for (const locale of readdirSync(LOCALES)) {
  const file = join(LOCALES, locale, "translation.json");
  const json = JSON.parse(readFileSync(file, "utf8"));
  const values = locale === "pt" ? PT : EN;
  let added = 0;
  for (const [dotted, text] of Object.entries(values)) {
    const parts = dotted.split(".");
    let node = json;
    for (const part of parts.slice(0, -1)) {
      if (typeof node[part] !== "object" || node[part] === null) node[part] = {};
      node = node[part];
    }
    const leaf = parts[parts.length - 1];
    if (!(leaf in node)) {
      node[leaf] = text;
      added++;
    }
  }
  writeFileSync(file, JSON.stringify(json, null, 2) + "\n");
  console.log(`${locale}: +${added}`);
}
```

Depois de rodar:
1. `bun run format:frontend`, para o Prettier normalizar os JSONs.
2. `git diff --stat src/i18n`: os 29 arquivos mudaram e só com linhas
   adicionadas. Se algum mostrar linhas **removidas** além da
   reformatação, pare e confira (o arquivo tinha um formato que o
   `JSON.stringify` mudou, como escapes `\u`). Nesse caso, desfaça aquele
   arquivo (`git checkout -- <arquivo>`) e adicione as chaves à mão.
3. `bun run check:translations`.
