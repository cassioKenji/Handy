# Tasks: gravação de reunião pela CLI (`--output-file`, `--start-transcription`, `recording.json`)

Este arquivo divide a implementação em tasks pequenas, em ordem. Cada task diz
quais arquivos mexer, o que escrever, como verificar e qual commit fazer.
**Siga a ordem.** Cada task depende das anteriores. O código citado aqui foi
conferido no fork; você não deve precisar "descobrir" o design.

---

## Contexto

Um script externo (a skill `start-transcription` do projeto
`call-transcription-concierge`) precisa:

1. abrir o Handy, ou falar com o Handy já aberto, e mandar gravar **neste
   arquivo** (um arquivo novo por reunião);
2. **só iniciar** a gravação: nunca parar uma que já está rodando;
3. saber, lendo um arquivo, se o Handy está gravando, em qual arquivo, de qual
   fonte de áudio e desde quando.

Hoje o arquivo de saída só existe como configuração persistida
(`output_to_file_enabled` + `output_file_path`), e o `--toggle-transcription`
alterna: ele pode **parar** uma gravação.

### O que será entregue

| Peça | Comportamento |
| --- | --- |
| `--output-file <caminho>` | Override **em memória** do arquivo de saída. Vale só para a próxima gravação: é descartado quando essa gravação termina (processamento concluído, cancelamento ou falha ao iniciar). Não altera as configurações salvas. Ignorado (com `warn!`) se chegar enquanto há gravação ou processamento em andamento. |
| `--start-transcription` | Inicia a gravação (binding `transcribe`) se nada estiver gravando. Se já houver gravação, não faz nada (`info!`). Se o pipeline estiver processando a gravação anterior, a nova começa assim que o processamento acabar. |
| `recording.json` | Existe só enquanto há uma sessão (gravação + processamento). Fica no diretório de dados do app (`portable::app_data_dir`); no build portable, é `Data/recording.json`, ao lado do `handy.exe`. |

Formato do `recording.json`:

```json
{
  "recording": true,
  "file": "D:\\reunioes\\transcriptions\\daily_261126_0930.txt",
  "since": "2026-11-26T09:30:12.123-03:00",
  "source": "system_output",
  "device": "Alto-falantes (Realtek)",
  "pid": 12345
}
```

- `file`: destino da transcrição, ou `null` quando ela vai ser colada (sem arquivo).
- `source`: `"microphone"` ou `"system_output"` (a serialização de `InputSource`).
- `device`: nome do device escolhido nas configurações, ou `null` para o padrão
  do sistema.
- `pid`: processo do Handy. Serve para o script detectar um arquivo órfão
  deixado por um crash.

### Decisões de design (já tomadas, não reabra)

- **Por que um override que vale uma gravação só:** o AGENTS.md exige que flags
  de CLI não alterem configurações persistidas. E, se o override ficasse até o
  app fechar, um ditado comum depois da reunião iria parar no arquivo da
  reunião.
- **Por que o override passa pelo coordinator:** o `TranscriptionCoordinator`
  processa os comandos em ordem, numa thread só. Mandar o "trocar arquivo" e o
  "iniciar" pelo mesmo canal garante que o arquivo é trocado antes de a
  gravação começar, e que a troca é recusada se já houver sessão.
- **Por que o status é escrito no coordinator:** é lá que ficam os três
  pontos de ciclo de vida: início confirmado (`run_effect` → `Effect::Start`
  com `started == true`), fim do processamento (`Command::ProcessingFinished`)
  e cancelamento (`Command::Cancel`).

---

## Regras gerais (leia antes de começar)

1. **Uma task por vez.** Termine a verificação antes de começar a próxima.
   Se uma verificação falhar, conserte antes de seguir.
2. **Não mexa em arquivos que a task não lista.** Se achar que precisa, pare e
   explique o motivo ao humano.
3. **Ache o código pelo texto, não pelo número da linha.** As linhas citadas
   ("~599") são aproximadas. Procure o trecho citado com `grep -n`.
4. **Nunca use `unwrap()` em código novo de produção.** Nos testes pode. Para
   `Mutex`, use `.lock().unwrap_or_else(|e| e.into_inner())`.
5. **Comentários e doc comments em inglês**, no estilo do arquivo: frases
   curtas que explicam o porquê.
6. **Onde rodar:** o build precisa de Rust e Bun no **Windows** (o WSL deste
   repo não tem cargo). A partir da raiz do repo:
   - `cd src-tauri && cargo check`, `cargo test`, `cargo clippy`
   - `bun run format` antes de cada commit
7. **Nenhuma task muda o frontend nem o `bindings.ts`.** Nenhum comando Tauri é
   criado ou alterado.
8. **Commits:** prefixos convencionais, mensagem focada no porquê. Use as
   mensagens sugeridas. Termine cada mensagem com a linha de co-autoria que o
   humano pedir.

---

## T1: Flags novas em `cli.rs`

**Arquivo:** `src-tauri/src/cli.rs`

**Passos:**

1. Em `CliArgs`, logo depois do campo `cancel`, adicione:

   ```rust
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
   ```

   `PathBuf` já está importado no arquivo.

2. No fim do arquivo, adicione uma função pura que resolve caminhos relativos.
   O segundo processo pode ter sido aberto em outra pasta, então o caminho
   relativo vale em relação ao **cwd de quem chamou**:

   ```rust
   /// Resolves `--output-file` against the caller's working directory, so a
   /// relative path means what it meant in the shell that ran the command.
   pub fn resolve_output_file(path: &Path, cwd: &Path) -> PathBuf {
       if path.is_absolute() {
           path.to_path_buf()
       } else {
           cwd.join(path)
       }
   }
   ```

   Troque o import para `use std::path::{Path, PathBuf};`.

3. Adicione testes no fim do arquivo:

   ```rust
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
           for other in ["--toggle-transcription", "--toggle-post-process", "--cancel"] {
               assert!(
                   CliArgs::try_parse_from(["handy", "--start-transcription", other]).is_err(),
                   "{other} should conflict"
               );
           }
       }

       #[test]
       fn relative_output_file_is_joined_to_cwd() {
           let cwd = Path::new("/work");
           assert_eq!(
               resolve_output_file(Path::new("t/a.txt"), cwd),
               PathBuf::from("/work/t/a.txt")
           );
       }

       #[test]
       fn absolute_output_file_is_kept() {
           let abs = std::env::temp_dir().join("a.txt");
           assert_eq!(resolve_output_file(&abs, Path::new("/work")), abs);
       }
   }
   ```

   Se já existir um `mod tests` em `cli.rs`, adicione os testes dentro dele.

**Verificação:** `cargo test cli::` passa e `cargo check` não gera warnings
novos. Os campos ainda não são usados em outro lugar; se o clippy reclamar de
código morto, ignore até a T5.

**Commit:** `feat(cli): add --output-file and --start-transcription flags`

---

## T2: Override do arquivo de saída em `output_file.rs`

**Arquivo:** `src-tauri/src/output_file.rs`

**Objetivo:** todo o código que hoje decide o destino chama
`output_file::destination(&settings)` ou `output_file::live_file_path(&settings)`:

- `actions.rs` ~599 (`live_to_file`);
- `actions.rs` ~945 (destino final no `stop`);
- `managers/transcription.rs` ~899;
- `managers/transcription/live.rs` ~67 (cria o `LiveFileSink`).

Fazendo `destination` levar o override em conta, **todos esses pontos passam a
respeitá-lo sem nenhuma mudança neles.** Não altere esses quatro arquivos.

**Passos:**

1. Adicione no topo, depois dos imports (`Mutex` vem de `std::sync`):

   ```rust
   use std::sync::Mutex;

   /// `--output-file`: the file for the next recording, kept in memory only
   /// (CLI flags never change persisted settings). The coordinator sets it
   /// while idle and clears it when that recording's session ends.
   static OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

   pub fn set_override(path: PathBuf) {
       *OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
   }

   pub fn clear_override() {
       *OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = None;
   }

   fn current_override() -> Option<PathBuf> {
       OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()).clone()
   }
   ```

2. Troque o corpo de `destination` por uma chamada a uma função pura nova, e
   mova a lógica atual para ela:

   ```rust
   pub fn destination(settings: &AppSettings) -> Destination {
       resolve(current_override().as_deref(), settings)
   }

   /// The override wins over settings, even with the switch off: the caller
   /// asked for this file explicitly.
   fn resolve(override_path: Option<&Path>, settings: &AppSettings) -> Destination {
       if let Some(path) = override_path {
           return Destination::File(path.to_path_buf());
       }
       if !settings.output_to_file_enabled {
           return Destination::Paste;
       }
       match settings.output_file_path.as_deref().map(str::trim) {
           Some(path) if !path.is_empty() => Destination::File(PathBuf::from(path)),
           _ => Destination::NoFile,
       }
   }
   ```

3. No `mod tests` existente, adicione os testes abaixo. **Eles testam `resolve`,
   nunca o `OVERRIDE` global:** os testes rodam em paralelo, e um teste que
   mexesse no global quebraria os outros.

   ```rust
   #[test]
   fn override_wins_over_paste() {
       assert_eq!(
           resolve(Some(Path::new("C:\\m.txt")), &settings(false, None)),
           Destination::File(PathBuf::from("C:\\m.txt"))
       );
   }

   #[test]
   fn override_wins_over_the_picked_file() {
       assert_eq!(
           resolve(Some(Path::new("C:\\m.txt")), &settings(true, Some("C:\\notes.txt"))),
           Destination::File(PathBuf::from("C:\\m.txt"))
       );
   }

   #[test]
   fn no_override_keeps_settings_behaviour() {
       assert_eq!(resolve(None, &settings(true, None)), Destination::NoFile);
   }
   ```

**Verificação:** `cargo test output_file::` passa, incluindo os testes antigos
(eles continuam chamando `destination`, e o global é `None` nos testes).

**Commit:** `feat(output): let a runtime override pick the output file`

---

## T3: Módulo `session_status.rs`

> O nome não é `recording_status.rs` porque o `.gitignore` do repo ignora
> `recording_*`.

**Arquivos:** `src-tauri/src/session_status.rs` (novo), `src-tauri/src/lib.rs`
(só a declaração do módulo)

**Passos:**

1. Em `lib.rs`, na lista de módulos, em ordem alfabética (entre
   `secure_input` e `settings`), adicione `mod session_status;`.

2. Crie `src-tauri/src/session_status.rs`:

   ```rust
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
       let json = serde_json::to_vec_pretty(status)
           .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
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
       let Some(path) = status_path(app) else { return };
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
       let Some(path) = status_path(app) else { return };
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
   ```

   Confira os caminhos dos imports com `grep`: `AudioRecordingManager` fica em
   `crate::managers::audio`, e `get_settings` e `InputSource` ficam em
   `crate::settings`. Se `InputSource` não implementar `PartialEq` para o
   `derive`, ele implementa: veja `#[derive(... PartialEq, Eq ...)]` em
   `settings.rs`.

**Verificação:** `cargo test session_status::` passa. As funções públicas
ainda não são chamadas; o aviso de código morto some na T4.

**Commit:** `feat: add recording.json so scripts can see the recording session`

---

## T4: Coordinator: comandos novos e ganchos de ciclo de vida

**Arquivo:** `src-tauri/src/transcription_coordinator.rs`

### 4.1 Comandos novos

No `enum Command` (~208), adicione duas variantes:

```rust
enum Command {
    Input(InputEvent),
    Cancel { recording_was_active: bool },
    ProcessingFinished,
    /// `--output-file`: applies only while nothing is recording or processing.
    SetOutputFile(std::path::PathBuf),
    /// `--start-transcription`: an external press that never stops a session.
    ExternalStart(InputEvent),
}
```

### 4.2 Lógica pura em `CoordinatorState`

Dentro de `impl CoordinatorState`, adicione:

```rust
/// Whether the output file may change now: a session in flight still
/// writes to the file it started with.
fn can_change_output_file(&self) -> bool {
    matches!(self.stage, Stage::Idle) && self.pending_press.is_none()
}

/// A start-only external press. While recording, or with a start already
/// remembered, a toggle press would stop or forget it, so it is ignored.
/// While processing it is remembered and starts once the pipeline drains.
fn on_external_start(&mut self, input: InputEvent, now: Instant) -> Option<Effect> {
    if matches!(self.stage, Stage::Recording(_)) || self.pending_press.is_some() {
        return None;
    }
    self.on_input(input, now)
}
```

### 4.3 Construção do evento externo

Hoje `send_external_input` monta o evento por dentro de `send`. Adicione na
`impl TranscriptionCoordinator` dois métodos novos, **sem mexer** em
`send_external_input`:

```rust
/// `--start-transcription`: start a recording, never stop one.
pub fn send_external_start(&self, binding_id: &str, source: &str) {
    let input = InputEvent {
        binding_id: binding_id.to_string(),
        hotkey_string: source.to_string(),
        is_pressed: true,
        mode: ShortcutActivation::Toggle,
        hold_threshold: Duration::ZERO,
        external: true,
        received_at: Instant::now(),
    };
    if self.tx.send(Command::ExternalStart(input)).is_err() {
        warn!("Transcription coordinator channel closed");
    }
}

/// `--output-file`: the file for the next recording.
pub fn send_output_file(&self, path: std::path::PathBuf) {
    if self.tx.send(Command::SetOutputFile(path)).is_err() {
        warn!("Transcription coordinator channel closed");
    }
}
```

Os valores do `InputEvent` são os mesmos que `send_external_input` usa hoje
(veja `send_external_input` → `send(binding_id, source, true, Toggle, ZERO, true)`).

### 4.4 Fim de sessão

Adicione uma função livre perto de `run_effect`:

```rust
/// A recording session ended: the CLI override was for that recording only,
/// and recording.json must not claim a session that is over.
fn end_session(app: &AppHandle) {
    crate::output_file::clear_override();
    crate::session_status::clear(app);
}
```

### 4.5 Ligar tudo no loop da thread

No `match cmd { ... }` dentro de `TranscriptionCoordinator::new`:

1. **`Command::Cancel`**: logo depois de `state.on_cancel(recording_was_active);`,
   dentro do `if busy { ... }` que já existe, adicione `end_session(&app);`
   antes do `emit_recording_phase`.

2. **`Command::ProcessingFinished`**: troque o braço por:

   ```rust
   Command::ProcessingFinished => {
       // A late finish after a cancel (stage already Idle) belongs to a
       // session that already ended; don't touch a newer one.
       if matches!(state.stage, Stage::Processing) {
           end_session(&app);
       }
       if let Some(effect) = state.on_processing_finished() {
           run_effect(&app, &mut state, effect);
       }
   }
   ```

   A ordem importa: `end_session` vem **antes** de `on_processing_finished`,
   porque este pode iniciar uma gravação lembrada, que vai escrever um
   `recording.json` novo.

3. **Novos braços:**

   ```rust
   Command::SetOutputFile(path) => {
       if state.can_change_output_file() {
           debug!("Output file for the next recording: {:?}", path);
           crate::output_file::set_override(path);
       } else {
           warn!("Ignoring --output-file {:?}: a recording session is running", path);
       }
   }
   Command::ExternalStart(input) => {
       let now = input.received_at;
       match state.on_external_start(input, now) {
           Some(effect) => run_effect(&app, &mut state, effect),
           None => info!("Ignoring --start-transcription: already recording"),
       }
   }
   ```

   Adicione `info` ao `use log::{...}` do topo. Observação: no caso
   "processando, sem press lembrada", `on_external_start` retorna `None`, mas
   guarda a press. Nesse caso a mensagem `info!` é enganosa. Para evitar,
   loge só quando o stage for `Recording`:

   ```rust
   None if matches!(state.stage, Stage::Recording(_)) => {
       info!("Ignoring --start-transcription: already recording")
   }
   None => debug!("--start-transcription remembered until processing ends"),
   ```

   (Se o press foi ignorado porque já havia um lembrado, cai no `debug!`. É
   aceitável.)

### 4.6 Início confirmado e falha de início

Em `run_effect`, no braço `Effect::Start`, troque:

```rust
let started = start(app, &binding_id, &hotkey_string);
state.on_start_result(&binding_id, started);
if started {
    emit_recording_phase(app, RecordingPhase::Started);
}
```

por:

```rust
let started = start(app, &binding_id, &hotkey_string);
state.on_start_result(&binding_id, started);
if started {
    crate::session_status::write_started(app);
    emit_recording_phase(app, RecordingPhase::Started);
} else {
    // Nothing will record: drop the override so a later dictation doesn't
    // land in that file.
    end_session(app);
}
```

### 4.7 Testes

No `mod tests` do arquivo existem os helpers `toggle_input(external: bool)` e
`drive_into_processing(state, now)`. Use-os. Adicione:

```rust
#[test]
fn external_start_from_idle_starts() {
    let mut state = CoordinatorState::new();
    let now = Instant::now();
    assert!(matches!(
        state.on_external_start(toggle_input(true), now),
        Some(Effect::Start { .. })
    ));
}

#[test]
fn external_start_while_recording_does_nothing() {
    let mut state = CoordinatorState::new();
    let now = Instant::now();
    state.on_external_start(toggle_input(true), now);
    let effect = state.on_external_start(toggle_input(true), now + Duration::from_secs(1));
    assert_eq!(effect, None);
    assert_eq!(state.stage, Stage::Recording(BINDING.to_string()));
}

#[test]
fn external_start_while_processing_starts_after_drain() {
    let mut state = CoordinatorState::new();
    let now = Instant::now();
    drive_into_processing(&mut state, now);
    assert_eq!(state.on_external_start(toggle_input(true), now), None);
    assert!(matches!(
        state.on_processing_finished(),
        Some(Effect::Start { .. })
    ));
}

#[test]
fn second_external_start_while_processing_keeps_the_first() {
    let mut state = CoordinatorState::new();
    let now = Instant::now();
    drive_into_processing(&mut state, now);
    state.on_external_start(toggle_input(true), now);
    state.on_external_start(toggle_input(true), now);
    assert!(state.pending_press.is_some());
    assert!(matches!(
        state.on_processing_finished(),
        Some(Effect::Start { .. })
    ));
}

#[test]
fn output_file_changes_only_when_idle() {
    let mut state = CoordinatorState::new();
    let now = Instant::now();
    assert!(state.can_change_output_file());
    state.on_external_start(toggle_input(true), now);
    assert!(!state.can_change_output_file());
}
```

Antes de escrever, abra `toggle_input` e `drive_into_processing` para conferir
as assinaturas e o nome da constante do binding (`BINDING`). Se `toggle_input`
usar outro binding, use o mesmo que ele usa no `assert_eq!` do stage. Se
`Effect` não derivar `PartialEq`, ele deriva (`#[derive(Debug, PartialEq, Eq)]`).

**Verificação:** `cargo test transcription_coordinator::` passa (testes antigos
e novos). `cargo clippy` sem warnings novos.

**Commit:** `feat(coordinator): start-only CLI input and session hooks for recording.json`

---

## T5: Ligar as flags na entrada do app

**Arquivos:** `src-tauri/src/signal_handle.rs`, `src-tauri/src/lib.rs`

### 5.1 Helpers em `signal_handle.rs`

Depois de `send_transcription_input`, adicione:

```rust
/// `--start-transcription`: start recording, never stop it.
pub fn start_transcription(app: &AppHandle, binding_id: &str, source: &str) {
    if let Some(c) = app.try_state::<TranscriptionCoordinator>() {
        c.send_external_start(binding_id, source);
    } else {
        warn!("TranscriptionCoordinator not initialized");
    }
}

/// `--output-file`: the file for the next recording.
pub fn set_output_file(app: &AppHandle, path: std::path::PathBuf) {
    if let Some(c) = app.try_state::<TranscriptionCoordinator>() {
        c.send_output_file(path);
    } else {
        warn!("TranscriptionCoordinator not initialized");
    }
}
```

### 5.2 Handy já aberto: callback do single-instance (`lib.rs` ~921)

O callback recebe `|app, args, _cwd|`. `args` é o `argv` completo do segundo
processo, com o caminho do executável na posição 0, então `CliArgs` consegue
parsear. Troque `_cwd` por `cwd` e adicione um bloco **no começo** do closure,
antes do `if args.iter().any(|a| a == "--toggle-transcription")`, transformando
o encadeamento atual em `else if`:

```rust
builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
    // --output-file / --start-transcription carry a value, so parse the
    // forwarded argv instead of matching strings.
    let parsed = <CliArgs as clap::Parser>::try_parse_from(&args).ok();
    let output_file = parsed.as_ref().and_then(|a| a.output_file.clone());
    let start = parsed.as_ref().is_some_and(|a| a.start_transcription);
    if output_file.is_some() || start {
        if let Some(path) = output_file {
            let path = cli::resolve_output_file(&path, std::path::Path::new(&cwd));
            signal_handle::set_output_file(app, path);
        }
        if start {
            signal_handle::start_transcription(app, "transcribe", "CLI");
        }
    } else if args.iter().any(|a| a == "--toggle-transcription") {
        // ... resto exatamente como está hoje ...
```

Observações:

- `set_output_file` vem **antes** de `start_transcription`: os dois entram na
  mesma fila do coordinator, então o arquivo é trocado antes de a gravação
  começar.
- `cwd` é `String` no plugin. Se o compilador indicar outro tipo, ajuste o
  `Path::new(&cwd)`.
- Se `try_parse_from` falhar (por exemplo, combinação proibida pela T1), o
  bloco novo é pulado e o comportamento antigo segue. É o que queremos.
- Confira se `cli` está acessível pelo nome (`pub mod cli;` está em `lib.rs`).
  Se `CliArgs` já estiver importado no topo de `lib.rs`, a sintaxe
  `<CliArgs as clap::Parser>` funciona sem `use` extra.

### 5.3 Handy fechado: fim do `setup` (`lib.rs` ~1163)

Ache o bloco com o comentário
`// Handy wasn't running, so no single-instance callback handles this flag`.
Imediatamente **antes** do `if cli_args.toggle_transcription {`, adicione:

```rust
// A crash may have left recording.json behind; this process owns it now.
session_status::clear(&app_handle);

if let Some(path) = cli_args.output_file.clone() {
    let cwd = std::env::current_dir().unwrap_or_default();
    signal_handle::set_output_file(&app_handle, cli::resolve_output_file(&path, &cwd));
}
```

E troque o encadeamento por:

```rust
if cli_args.start_transcription {
    signal_handle::start_transcription(&app_handle, "transcribe", "CLI");
} else if cli_args.toggle_transcription {
    // ... como está ...
} else if cli_args.toggle_post_process {
    // ... como está ...
}
```

`unwrap_or_default()` não é `unwrap()`: se não houver cwd, um caminho relativo
fica relativo, e o erro de escrita aparece como o notice `OutputFileFailed`
que já existe.

### 5.4 Apagar o status ao sair (`lib.rs` ~1216)

No `app.run(|app, event| match &event { ... })`:

1. Adicione um braço antes do `_ => {}`:

   ```rust
   tauri::RunEvent::Exit => {
       // A session cut short by quitting must not look alive to scripts.
       session_status::clear(app);
   }
   ```

2. A linha acima do `app.run`,
   `#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]`, existia
   porque `app` só era usado no macOS. Agora ele é usado sempre: **apague essa
   linha** e o comentário `// \`app\` is only used by the macOS arm.` logo acima
   dela.

**Verificação:**

- `cargo check`, `cargo test`, `cargo clippy` limpos.
- Teste manual no Windows, com `bun run tauri dev` **ou** com o build da T7
  (prefira o build, porque o `dev` pode não repassar argv do mesmo jeito):
  1. Handy fechado: `handy.exe --start-hidden --output-file C:\tmp\a.txt --start-transcription`.
     Ele abre gravando, `C:\tmp\a.txt` é criado vazio e `Data\recording.json`
     existe com `file` = `C:\tmp\a.txt`.
  2. Fale ou toque um áudio: o texto vai entrando em `a.txt`.
  3. Rode de novo `handy.exe --output-file C:\tmp\b.txt --start-transcription`:
     nada muda. Continua gravando em `a.txt`, `b.txt` não é criado e o log
     mostra `Ignoring --output-file` e `Ignoring --start-transcription`.
  4. Pare pelo atalho. Quando o processamento termina, o `recording.json` some.
  5. Grave pelo atalho, sem CLI: o destino volta ao das configurações (o
     override foi descartado). O `recording.json` aparece com esse destino, ou
     com `file: null` se for colar.
  6. Com uma gravação rodando, feche o Handy pelo tray: o `recording.json` some.
  7. `handy.exe --start-transcription --toggle-transcription` sai com erro de
     uso do clap e não faz nada.

**Commit:** `feat: wire --output-file and --start-transcription into launch and single-instance`

---

## T6: Documentação

**Arquivo:** `AGENTS.md`

1. Na tabela de **CLI Parameters**, depois da linha de `--cancel`, adicione:

   ```
   | `--start-transcription`  | Start recording, never stop it; does nothing while recording |
   | `--output-file <FILE>`   | Write the next recording to FILE (runtime only, one recording) |
   ```

   Ajuste o alinhamento da tabela ao das linhas vizinhas.
2. Em **Key design decisions**, adicione um item:

   ```
   - While a recording session runs, `recording.json` in the app data folder holds its file, source, device, start time and pid; it is removed when the session ends and on exit
   ```

**Verificação:** `bun run format:check` passa.

**Commit:** `docs: document --start-transcription, --output-file and recording.json`

---

## T7: Build portable e entrega

1. No Windows, gere o build: `bun run tauri build`. Siga o
   [BUILD.md](../../../BUILD.md) se faltar algo.
2. Monte o portable do mesmo jeito do build atual
   (`Handy-0.10.2-fork.1-windows-x64-portable.zip`): `handy.exe`, as DLLs
   `ggml*.dll` e `transcribe.dll`, o arquivo `portable` e `resources/`.
3. No projeto `call-transcription-concierge`, substitua os arquivos em `handy/`
   **sem apagar `handy/Data/`** (ali ficam as configurações e o modelo baixado).
4. Repita o teste manual da T5 usando `handy/handy.exe` e confirme que o
   `recording.json` aparece em `handy/Data/`.

Sem commit no fork. No projeto principal, não versione os binários (o
`.gitignore` já ignora `handy/*`).
