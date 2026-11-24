# Design: captura do áudio de saída do sistema (loopback)

Status: rascunho · 2026-11-24 · Spec: [spec.md](spec.md)

A spec diz **o que** entregar. Este documento diz **como**: os tipos, os
fluxos, onde cada decisão fica no código e por quê. As seções seguem a ordem
de implementação da spec (§7).

---

## 1. Visão geral

```
┌──────────────────────── Frontend (React) ─────────────────────────┐
│ GeneralSettings › Sound                                           │
│   InputSourceSelector ──► input_source                            │
│   ├─ microphone:    MicrophoneSelector, ChannelSelector, Mute…    │
│   └─ system_output: SystemOutputSelector ──► selected_loopback_…  │
│ settingsStore: loopbackDevices, refreshLoopbackDevices            │
└───────────────┬───────────────────────────────────────────────────┘
                │ tauri commands (bindings.ts)
┌───────────────▼──────────── Handy (src-tauri) ────────────────────┐
│ commands/audio.rs   set_input_source, get_available_loopback_…    │
│ settings.rs         InputSource, selected_loopback_device         │
│ managers/audio.rs   DesiredDevice ─► resolve ─► RecorderConfig    │
│                     OpenMicrophone { kind }  ◄── fonte da verdade │
│ actions.rs          sons de aviso / notices condicionados ao kind │
└───────────────┬───────────────────────────────────────────────────┘
                │ handy_recorder::{list_loopback_devices, Recorder}
┌───────────────▼──────── crates/handy-recorder (fork) ─────────────┐
│ device.rs        DeviceKind, list_loopback_devices, loopback_…    │
│ backend/mod.rs   Backend::{list_loopback_devices, …}              │
│ backend/cpal     enumerate(Direction), open_device("loopback:…")  │
│                  keepalive silencioso (Windows)                   │
│ capture/engine   checagens de permissão de mic ignoradas p/ loop. │
└───────────────┬───────────────────────────────────────────────────┘
                │ build_input_stream(device de SAÍDA)
┌───────────────▼──────── handy-cpal 0.18.2 (sem mudanças) ─────────┐
│ WASAPI: AUDCLNT_STREAMFLAGS_LOOPBACK  ·  CoreAudio: process tap   │
└───────────────────────────────────────────────────────────────────┘
```

**Princípio central:** a decisão "isto é loopback?" é tomada **uma vez**, na
resolução do device. Depois disso ela anda junto com o recorder aberto
(`InputDevice::kind`). Mute, sons de aviso e notices consultam o **kind do
recorder aberto**, nunca o setting. Assim, mudar o setting no meio de uma
gravação não deixa o estado incoerente (ver §4.4).

---

## 2. `crates/handy-recorder`

### 2.1 Vendoring

```
crates/handy-recorder/      ← cópia exata do 0.1.1 (commit 1)
src-tauri/Cargo.toml:
-handy-recorder = "0.1.1"
+handy-recorder = { path = "../crates/handy-recorder" }
```

- Manter o `version = "0.1.1"` no `Cargo.toml` da crate e mudar para
  `0.1.1-loopback.N` só quando a API mudar. Assim o `Cargo.lock` mostra que é
  o fork.
- O `Cargo.toml` vindo do crates.io é "normalizado" pelo cargo publish. Ele
  funciona como está e não precisa ser reescrito.
- `crates/handy-recorder/README.md` ganha uma seção "Fork" listando os
  arquivos tocados, para facilitar o merge com versões futuras.

### 2.2 API pública

```rust
// device.rs

/// What an `InputDevice` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum DeviceKind {
    #[default]
    Microphone,
    /// A sound card's output, captured by loopback.
    Loopback,
}

pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub occurrence: u32,
    pub backend: String,
    pub is_default: bool,      // Loopback: a saída padrão do sistema
    pub id_is_stable: bool,
    pub channels: Option<u16>, // Loopback: canais da config de saída
    pub is_monitor: bool,
    pub kind: DeviceKind,      // NOVO
}

/// Microphones only. Unchanged.
pub fn list_input_devices() -> Result<Vec<InputDevice>, Error>;

/// Outputs that can be recorded by loopback. Empty where unsupported.
pub fn list_loopback_devices() -> Result<Vec<InputDevice>, Error>;

/// Whether this OS can record its output at all (macOS ≥ 14.6, Windows).
pub fn loopback_supported() -> bool;
```

- Como `InputDevice` é `#[non_exhaustive]`, o campo novo não quebra quem só
  lê o struct.
- `RecorderConfig` **não muda**: o tipo do device vem do id (§2.3). Não
  existe "loopback padrão" via `device: None`. O `None` continua significando
  o microfone padrão, e o chamador resolve a saída padrão com
  `list_loopback_devices()` (§3.3).

### 2.3 Esquema de IDs

```rust
// backend/mod.rs
pub(crate) const LOOPBACK_PREFIX: &str = "loopback:";

pub(crate) enum DeviceRef<'a> {
    Default,             // None
    Input(&'a str),      // "<cpal id>"
    Loopback(&'a str),   // "loopback:<cpal id>"
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

Por que um prefixo, e não um campo em `RecorderConfig`:

- O CoreAudio usa **o mesmo UID** para a entrada e a saída de uma placa, e o
  `cpal::DeviceId` sozinho não diz a direção.
- O Handy guarda e cacheia ids como `String` (`cached_device`). Um id que já
  carrega a direção não deixa um id de mic ser reusado como saída por engano.
- A `Backend::open_device(id)` não muda de assinatura.

O `Display` do `cpal::DeviceId` é `"<host>:<id>"` (ex.: `wasapi:{0.0.0…}`),
então o id final fica `loopback:wasapi:{…}`. O parse só remove o primeiro
prefixo.

### 2.4 Trait `Backend`

```rust
pub(crate) trait Backend: Send + Sync + 'static {
    fn list_input_devices(&self) -> Result<Vec<InputDevice>, BackendError>;
    fn list_loopback_devices(&self) -> Result<Vec<InputDevice>, BackendError>; // NOVO
    fn loopback_supported(&self) -> bool;                                      // NOVO
    fn open_device(&self, id: Option<&str>) -> Result<Box<dyn OpenDevice>, BackendError>;
    fn permission_status(&self) -> Permission;
    fn denial_is_silent(&self) -> bool;
}
```

`list_loopback_devices` em `device.rs` chama
`CpalBackend::shared().list_loopback_devices()`, no mesmo padrão de
`list_input_devices`.

### 2.5 `backend/cpal/mod.rs`

#### Enumeração

`enumerate()` ganha uma direção:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction { Input, Loopback }

fn enumerate(host: &cpal::Host, dir: Direction, with_channels: bool)
    -> Result<Vec<(InputDevice, cpal::Device)>, BackendError>
{
    let (devices, default_id) = match dir {
        Direction::Input => (host.input_devices(), host.default_input_device()),
        Direction::Loopback => (host.output_devices(), host.default_output_device()),
    };
    // ... laço atual: occurrence, null PCM, nomes ...
    // Loopback:
    //   id         = format!("{LOOPBACK_PREFIX}{cpal_id}")
    //   kind       = DeviceKind::Loopback
    //   is_monitor = false
    //   channels   = device.default_output_config().ok().map(|c| c.channels())
    //   pular se !loopback_capable(&device)   (macOS, abaixo)
}
```

`loopback_capable(device)`:

| Plataforma     | Regra                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Windows        | sempre `true`                                                                                                                         |
| macOS          | `device.default_input_config().is_err()`: só devices **sem** entrada. Com entrada, o cpal abriria o mic (`supports_input()`; spec R1) |
| Linux / outros | `false`. A lista sai vazia na v1                                                                                                      |

O `list_loopback_devices()` do backend retorna `Ok(vec![])` direto quando
`!self.loopback_supported()`, sem enumerar nada.

#### `loopback_supported()`

| Plataforma | Implementação                                                                                                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows    | `true`                                                                                                                                                                                                  |
| macOS      | `NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion({14, 6, 0})`. Precisa de `objc2-foundation` com a feature `NSProcessInfo` como dep macOS da crate (o Handy já usa `objc2-foundation 0.3`) |
| Linux      | `false`                                                                                                                                                                                                 |

Calcular uma vez e guardar num `OnceLock<bool>`.

#### `open_device`

```rust
fn open_device(&self, id: Option<&str>) -> Result<Box<dyn OpenDevice>, BackendError> {
    let host = self.host()?;
    let (info, device, config) = match DeviceRef::parse(id) {
        DeviceRef::Default    => /* caminho atual: default input */,
        DeviceRef::Input(id)  => /* caminho atual, enumerate(Input) */,
        DeviceRef::Loopback(raw) => {
            if !self.loopback_supported() {
                return Err(BackendError::new(UnsupportedConfig,
                    "Recording system audio needs macOS 14.6 or later"));
            }
            let full = format!("{LOOPBACK_PREFIX}{raw}");
            let (info, device) = enumerate(&host, Direction::Loopback, false)?
                .into_iter().find(|(i, _)| i.id == full)
                .ok_or_else(|| BackendError::new(DeviceNotAvailable,
                    format!("No output device with ID {raw:?}")))?;
            let config = device.default_output_config()        // ← ponto-chave
                .map_err(|e| map_error_during("Failed to fetch output config", e))?;
            (info, device, config)
        }
    };
    // ... mapeamento de SampleFormat atual, comum aos três ramos ...
    Ok(Box::new(CpalOpenDevice { kind: info.kind, /* ... */ }))
}
```

O mapeamento de `SampleFormat` e a montagem do `CpalOpenDevice` saem para uma
função comum, `finish_open(host, info, device, config)`, para que os três
ramos não repitam código.

#### Keepalive (Windows)

O problema: o WASAPI loopback só manda pacotes enquanto algum app renderiza
áudio naquele endpoint. Em silêncio o callback não roda, e o watchdog derruba
o recorder com `NoAudio` (10 s sem o primeiro chunk) ou `Stalled` (5 s sem
chunks). A solução é um stream de **render silencioso** no mesmo endpoint
enquanto o loopback estiver aberto. O loopback então entrega zeros sem parar.

```rust
struct CpalStream {
    _stream: cpal::Stream,
    device_id: Option<cpal::DeviceId>,
    headset: Option<cpal::Stream>,      // macOS (AirPods), atual
    keepalive: Option<cpal::Stream>,    // NOVO: render silencioso p/ loopback
    #[cfg(target_os = "linux")] watch: Option<pulse::SourceWatch>,
}
```

- `silent_output(id)` passa de `#[cfg(target_os = "macos")]` para
  `#[cfg(any(target_os = "macos", target_os = "windows"))]`, sem outras
  mudanças. A mix format do WASAPI em modo shared é F32, e zero é silêncio.
- Em `CpalOpenDevice::start`, quando `self.kind == Loopback` e o SO é
  Windows: abrir o keepalive **antes** do stream de captura. Assim o
  primeiro pacote chega rápido e o `RecordingReadiness` dispara sem demora.
- Se o keepalive falhar: `log::warn!` e seguir sem ele. A captura funciona
  enquanto houver áudio tocando, e em silêncio o watchdog derruba o recorder
  com `NoAudio`/`Stalled`, que vira notice (§3.6). Preferimos degradar a
  impedir a gravação.
- Ordem no `Drop`: os campos caem na ordem de declaração. Pôr `_stream`
  antes de `keepalive`, para que a captura pare antes do render.
- macOS: `keepalive` fica `None` na v1. Se o critério de aceite 2 falhar no
  Mac (spec R3), ativar o mesmo caminho por `cfg`.

O buffer de 500 ms do WASAPI (`config.buffer_size = Fixed(sr/2)`) vale também
para o loopback, que roda no mesmo ramo `#[cfg(windows)]`.

### 2.6 `capture/engine.rs`: permissões

Três pontos usam a permissão de **microfone**. No loopback ela não se aplica:

| Linha (0.1.1)        | Hoje                                                        | Com loopback                                             |
| -------------------- | ----------------------------------------------------------- | -------------------------------------------------------- |
| `open` ~631          | `denial_is_silent && Denied` → `PermissionDenied`           | pular se `info.kind == Loopback`                         |
| `Engine::start` ~381 | permissão revogada com o stream aberto → `start` falha      | pular se for loopback                                    |
| `runtime_error` ~601 | erro de stream no Windows + mic negado → `PermissionDenied` | o mic negado não explica a falha: manter o kind original |

No `open_stream`, o `info` já existe. O `Engine::start` e o `runtime_error`
leem `shared.device.get().map(|d| d.kind)`.

### 2.7 Fake backend e testes

`backend/fake.rs`:

- `FakeBackend` ganha `loopback: Mutex<Vec<InputDevice>>` e
  `loopback_supported: AtomicBool`.
- `open_device` com `DeviceRef::Loopback` procura nessa lista e devolve um
  `FakeOpenDevice` com `kind = Loopback`.

Testes novos (`src/tests/loopback.rs`):

1. `list_input_devices` nunca contém um `Loopback`, e
   `list_loopback_devices` só contém `Loopback`.
2. Abrir `loopback:x` resolve o device de loopback mesmo havendo um mic com o
   mesmo id cru `x`.
3. `loopback_supported == false`: a lista sai vazia e o open dá
   `UnsupportedConfig`.
4. Com permissão de mic `Denied` e `denial_is_silent`, abrir loopback dá `Ok`
   e o watchdog não derruba o stream.
5. A teardown não trava, usando os gates de `hang_teardown`.

Hardware (`#[ignore]`): estender `lists_and_resolves_real_devices` para
listar e abrir cada loopback e imprimir o `format()`.

---

## 3. Handy (`src-tauri`)

### 3.1 Settings

```rust
// settings.rs
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    #[default]
    Microphone,
    SystemOutput,
}

pub struct AppSettings {
    // ...
    #[serde(default)]
    pub input_source: InputSource,
    /// Name of the output to record; None = the system's default output.
    #[serde(default)]
    pub selected_loopback_device: Option<String>,
}
```

- Atualizar os defaults em `get_default_settings()` e o JSON de teste
  (`settings.rs` ~1613).
- Não precisa de migração: `serde(default)`.
- Persistir **nomes**, não ids: o id do WASAPI muda entre drivers, e o
  `selected_microphone` também guarda o nome.

### 3.2 Comandos

```rust
// commands/audio.rs
#[tauri::command] #[specta::specta]
pub fn get_input_source(app: AppHandle) -> InputSource;

#[tauri::command] #[specta::specta]
pub async fn set_input_source(app: AppHandle, source: InputSource) -> Result<(), String>;
//  persiste → spawn_blocking(rm.update_selected_device())   (mesmo padrão
//  de set_selected_microphone)

#[tauri::command] #[specta::specta]
pub async fn get_available_loopback_devices() -> Result<Vec<AudioDevice>, String>;
//  spawn_blocking(list_loopback_devices) com "Default" na frente

#[tauri::command] #[specta::specta]
pub async fn set_selected_loopback_device(app: AppHandle, device_name: String) -> Result<(), String>;
#[tauri::command] #[specta::specta]
pub fn get_selected_loopback_device(app: AppHandle) -> Result<String, String>;

#[tauri::command] #[specta::specta]
pub fn is_loopback_supported() -> bool;
```

- `set_selected_loopback_device` só reabre o stream
  (`update_selected_device`) se `input_source == SystemOutput`. No modo mic,
  trocar a saída não toca no stream aberto.
- Registrar os seis em `lib.rs` (`collect_commands!`, junto de
  `get_available_microphones`).
- Os settings também chegam pelo updater genérico do `settingsStore` (§5.2).
  Se ele usar um comando `change_*_setting`, seguir o padrão existente; os
  comandos acima são os que reabrem o stream.

### 3.3 Resolução do device (`managers/audio.rs`)

#### Tipos

```rust
/// O dispositivo que as settings pedem agora.
enum DesiredDevice {
    DefaultMicrophone,
    Microphone(String),        // selected_microphone
    Clamshell(String),         // clamshell_microphone
    DefaultLoopback,           // NOVO
    Loopback(String),          // NOVO: selected_loopback_device
}

/// Chave do cache: o mesmo nome em kinds diferentes não colide.
#[derive(PartialEq, Eq, Clone)]
struct CacheKey { kind: DeviceKind, name: String }

cached_device: Arc<Mutex<Option<(CacheKey, String /* id */)>>>,

struct DeviceResolution {
    device: Option<String>,          // id para RecorderConfig
    kind: DeviceKind,
    from_cache: bool,
    /// Nome que o usuário escolheu e que sumiu (só quando a enumeração deu certo).
    unavailable: Option<(DeviceKind, String)>,
}
```

Renomes: `DesiredMicrophone` vira `DesiredDevice` e `MicrophoneResolution`
vira `DeviceResolution`. São privados e só usados nesse arquivo.

#### `desired_device(settings)`

```rust
fn desired_device(&self, s: &AppSettings) -> DesiredDevice {
    if s.input_source == InputSource::SystemOutput {
        return match &s.selected_loopback_device {
            Some(n) => DesiredDevice::Loopback(n.clone()),
            None => DesiredDevice::DefaultLoopback,
        };
    }
    // ... lógica atual (clamshell → selected → default) ...
}
```

O loopback vem **antes** do clamshell: o `ioreg` nem roda no modo saída.

#### `resolve_device(settings)`

```
DefaultMicrophone → { device: None, kind: Microphone }          (sem cache, como hoje)
Microphone/Clamshell(name) → caminho atual, com CacheKey{Microphone,name}

DefaultLoopback →
    list_loopback_devices()
      Ok(ds) → ds.find(is_default)                → { Some(id), Loopback }
               nenhum is_default → ds.first()     → idem (log warn)
               lista vazia                         → Err(DeviceUnavailable)  (ver abaixo)
      Err(e) → Err(e)   (não há fallback "None" seguro)
    sem cache: a saída padrão muda com frequência

Loopback(name) →
    cache hit CacheKey{Loopback,name}        → { Some(id), Loopback, from_cache }
    list_loopback_devices().find(name == )   → { Some(id), Loopback } + cache
    não achou (enumeração ok)                 → resolve DefaultLoopback
                                                + unavailable = (Loopback, name)
```

**Sem device de loopback**: `device: None` abriria o **microfone** padrão, o
que contraria o que o usuário escolheu e grava o mic sem ele saber. Por isso
`resolve_device` passa a retornar `Result<DeviceResolution, CaptureSourceError>`,
com um erro **do Handy**, porque o `handy_recorder::Error::new` é `pub(crate)`:

```rust
#[derive(Debug)]
pub enum CaptureSourceError {
    /// Nenhuma saída gravável (lista vazia ou a enumeração falhou).
    NoOutputDevice(String),
    /// O SO não grava a saída (macOS < 14.6).
    Unsupported,
}
```

O `?` em `start_microphone_stream` o embrulha num `anyhow::Error`, e o
`recording_notice_code` o recupera com `downcast_ref` (§3.6). A checagem de
`loopback_supported()` acontece aqui, antes de enumerar. Por isso o
`UnsupportedConfig` da crate (que o `open_error` transforma em
`ErrorKind::Backend`) não precisa ser decodificado no Handy.

**Tipo da fonte no Handy**: em vez do `DeviceKind` da crate, que é
`#[non_exhaustive]` e exige um braço `_` em todo `match`, o Handy usa o
próprio `InputSource` (`Microphone | SystemOutput`) como "kind" em
`DeviceResolution`, `CacheKey` e `OpenMicrophone`.

**Testabilidade**: o `InputDevice` também é `#[non_exhaustive]` e não pode ser
construído fora da crate. A função pura `pick_device` recebe
`&[DeviceEntry { id, name, is_default }]`, um struct do Handy convertido a
partir do `InputDevice`.

#### Fallback persistido

`persist_default_microphone_after_fallback(name)` passa a ser
`persist_default_after_fallback(kind, name)`:

| kind       | setting zerado             | evento `settings-changed`                               |
| ---------- | -------------------------- | ------------------------------------------------------- |
| Microphone | `selected_microphone`      | `{setting:"selected_microphone", value:"Default"}`      |
| Loopback   | `selected_loopback_device` | `{setting:"selected_loopback_device", value:"Default"}` |

A guarda continua a mesma: só zera se o setting atual ainda for `name`.

#### `start_microphone_stream`

Só três mudanças:

```rust
let resolution = self.resolve_device(&settings)?;
let (channel, take_headset) = match resolution.kind {
    DeviceKind::Loopback => (None, false),          // ignora channel/AirPods
    _ => (*self.selected_channel.lock().unwrap(),
          self.airpods_mode.load(Ordering::Acquire)),
};
// open_microphone_on_channel(...) e retry por cache: sem mudanças
*self.recorder.lock().unwrap() = Some(OpenMicrophone {
    kind: resolution.kind, recorder, failed,
});
```

`OpenMicrophone` ganha `kind: DeviceKind`, igual a `recorder.info().device.kind`.
O campo fica guardado para não ler o lock do recorder no caminho quente.

### 3.4 O kind aberto como fonte da verdade

```rust
impl AudioRecordingManager {
    /// O stream aberto (ou o último aberto) grava a saída do sistema.
    pub fn capturing_system_output(&self) -> bool {
        self.open_kind.load(Ordering::Acquire) == KIND_LOOPBACK
    }
}
// open_kind: Arc<AtomicU8>, gravado em start_microphone_stream e lido sem lock.
```

Por que não ler `settings.input_source`: o usuário pode trocar a fonte
durante uma gravação. O stream atual continua sendo o loopback até reabrir. O
mute, os sons e os notices daquela sessão têm que seguir o que está **de fato**
gravando.

`last_input_silent` ganha um par, `last_input_kind: Arc<AtomicU8>`, gravado
em `collect_recording` junto com o `input_silent`. É o kind da gravação que
terminou, e o `actions.rs` o lê depois do `stop_recording`.

### 3.5 Mute e sons de aviso

`apply_mute()`:

```rust
if !settings.mute_while_recording || self.capturing_system_output() { return; }
```

`remove_mute()` não muda: ele só desfaz o que foi aplicado.

Sons de aviso, em `actions.rs`. Eles tocam em dois lugares:

| Linha                             | Hoje                                  | Novo                                     |
| --------------------------------- | ------------------------------------- | ---------------------------------------- |
| ~664 (start, depois da readiness) | `play_feedback_sound_blocking(Start)` | `if !rm.capturing_system_output() { … }` |
| ~736 (stop)                       | `play_feedback_sound(Stop)`           | idem                                     |

O `play_test_sound` em Settings não muda: ele não grava nada.

**Por que nas chamadas e não dentro de `audio_feedback`:** o
`audio_feedback` não conhece o manager e é usado também pelo teste de som. A
regra "não tocar enquanto grava a saída" é sobre a sessão de gravação, e
quem conhece a sessão é o `actions.rs`.

### 3.6 Notices

Novos `NoticeCode`, seguindo o passo a passo do AGENTS.md:

| Code                     | Level   | Quando                                                                                    | Título (en)                  |
| ------------------------ | ------- | ----------------------------------------------------------------------------------------- | ---------------------------- |
| `SystemAudioSilent`      | Warning | gravação loopback que só teve zeros                                                       | "Nothing Was Playing"        |
| `NoOutputDevice`         | Error   | loopback sem device / device sumiu sem fallback                                           | "No Output Device"           |
| `SystemAudioUnsupported` | Error   | `UnsupportedConfig` em loopback (macOS < 14.6, chamado pela CLI ou por um setting antigo) | "System Audio Not Supported" |

Explicações (`errors.<name>` sem o sufixo `Title`):

- `systemAudioSilent`: "Handy recorded your sound output, but nothing was
  playing." Mais uma variante por plataforma, no padrão de
  `micPermissionDenied.<platform>`: no macOS, "If something was playing,
  allow Handy under Privacy & Security › System Audio Recording."
- `noOutputDevice`: "Connect speakers or headphones, or switch back to the
  microphone in Settings."

Mudanças de assinatura:

```rust
// managers/audio.rs
pub fn recording_notice_code(error: &anyhow::Error, source: InputSource) -> NoticeCode {
    if let Some(e) = error.downcast_ref::<CaptureSourceError>() {
        return match e {
            CaptureSourceError::NoOutputDevice(_) => NoticeCode::NoOutputDevice,
            CaptureSourceError::Unsupported => NoticeCode::SystemAudioUnsupported,
        };
    }
    let kind = error.downcast_ref::<handy_recorder::Error>().map(handy_recorder::Error::kind);
    match source {
        InputSource::SystemOutput => match kind {
            Some(ErrorKind::DeviceUnavailable) => NoticeCode::NoOutputDevice,
            _ => NoticeCode::RecordingFailed,
        },
        InputSource::Microphone => /* match atual */,
    }
}
```

- Em `actions.rs` ~688, a chamada passa o kind **desejado**
  (`rm.desired_kind()`, derivado das settings), porque a falha aconteceu
  antes de qualquer stream abrir.
- `finish_without_text(app, input_silent, input_kind, …)`: com `input_silent`,
  escolhe `SystemAudioSilent` se `input_kind == Loopback` e
  `MicrophoneSilent` caso contrário.
- Os testes existentes de `recording_notice_code` (`audio.rs` ~1389) ganham
  o argumento `DeviceKind::Microphone`, e entram casos novos de `Loopback`.

---

## 4. Fluxos

### 4.1 Trocar a fonte

```
UI InputSourceSelector ─ updateSetting("input_source","system_output")
 └► commands::set_input_source
     ├ write_settings(input_source = SystemOutput)
     └ spawn_blocking: rm.update_selected_device()
         ├ invalidate_device_cache()
         └ se aberto: stop_microphone_stream() → start_microphone_stream()
             └ resolve_device → DefaultLoopback → list_loopback_devices
                 → Recorder::open("loopback:wasapi:{…}")
                     → default_output_config → keepalive → build_input_stream
```

O `update_selected_device` não checa se há gravação em andamento, e trocar
de fonte no meio de uma gravação a descarta. É o mesmo comportamento de hoje
para a troca de microfone, mantido de propósito.

### 4.2 Gravar com nada tocando (Windows)

```
atalho ↓ → try_start_recording → start_microphone_stream (já aberto ou abre)
  keepalive render (zeros) ─► WASAPI loopback entrega pacotes de zeros
  primeiro chunk → RecordingReadiness → overlay "recording"
  (sem som de início: capturing_system_output)
  (sem mute)
atalho ↑ → stop_recording → collect_recording
  input_silent = true, last_input_kind = Loopback
  → finish_without_text → notice SystemAudioSilent no overlay
```

### 4.3 Gravar um vídeo

O fluxo é o mesmo de 4.2 até o stop. Depois `input_silent = false`, VAD →
transcrição → colagem, igual ao microfone.

---

## 5. Frontend

### 5.1 Componentes

```
src/components/settings/
  InputSourceSelector.tsx     NOVO
  SystemOutputSelector.tsx    NOVO
  general/GeneralSettings.tsx renderização condicional
```

`InputSourceSelector`:

- Usar o `Dropdown` de `ui/` com duas opções. É o padrão visual dos
  seletores da seção, e um segmented control seria um componente novo sem
  precedente.
- Fica desabilitado quando `isLoopbackSupported` é falso, com a descrição
  `settings.sound.inputSource.unsupported` no tooltip.
- Faz `updateSetting("input_source", value)`.

`SystemOutputSelector`: copia a estrutura de `MicrophoneSelector`
(`SettingContainer` + `Dropdown` + `ResetButton`), trocando:

- `audioDevices` por `loopbackDevices` e `refreshAudioDevices` por
  `refreshLoopbackDevices`;
- a chave `selected_microphone` por `selected_loopback_device`.

`GeneralSettings.tsx` (seção Sound):

```tsx
const source = getSetting("input_source") ?? "microphone";
<InputSourceSelector … />
{source === "microphone" ? (
  <>
    <MicrophoneSelector … />
    <ChannelSelector … />
    <MuteWhileRecording … />
  </>
) : (
  <SystemOutputSelector … />
)}
<AudioFeedback … />          {/* fica; em loopback ganha a nota "não toca ao gravar a saída" */}
<OutputDeviceSelector … />
```

O `ClamshellMicrophoneSelector` está em `DebugSettings`: fica lá e é
ignorado no modo saída (o backend já ignora).

### 5.2 Store (`stores/settingsStore.ts`)

- O estado ganha `loopbackDevices: AudioDevice[]` e
  `loopbackSupported: boolean`.
- Ações novas: `refreshLoopbackDevices()`, uma cópia de
  `refreshAudioDevices` com `commands.getAvailableLoopbackDevices()`. O
  `loopbackSupported` é carregado uma vez em `initialize`.
- Updaters, no mapa onde já está `selected_microphone`:
  - `input_source: (v) => commands.setInputSource(v)`
  - `selected_loopback_device: (v) => commands.setSelectedLoopbackDevice(v === "Default" ? "default" : v)`
- Normalização em `loadSettings`:
  `selected_loopback_device: settings.selected_loopback_device ?? "Default"`.
- Listener `settings-changed` (~710): tratar `selected_loopback_device` como
  já se trata `selected_microphone`.

### 5.3 i18n

As chaves novas vão em `en/translation.json`, e depois todas entram nos
outros locales (`bun run check:translations`):

```
settings.sound.inputSource.title          "Input Source"
settings.sound.inputSource.description    "Record from a microphone, or from what your computer is playing"
settings.sound.inputSource.microphone     "Microphone"
settings.sound.inputSource.systemOutput   "System Output"
settings.sound.inputSource.unsupported    "Recording system audio needs macOS 14.6 or later"
settings.sound.systemOutput.title         "Output to Record"
settings.sound.systemOutput.description   "Which speakers or headphones to record"
settings.sound.systemOutput.placeholder   "Select output..."
settings.sound.audioFeedback.loopbackNote "Sounds are skipped while recording system output"
errors.systemAudioSilentTitle / systemAudioSilent(.macos)
errors.noOutputDeviceTitle / noOutputDevice
errors.systemAudioUnsupportedTitle / systemAudioUnsupported
```

O `build.rs` gera `strings.*_title` para a tray a partir das chaves com o
sufixo `Title`.

---

## 6. macOS

- **`src-tauri/Info.plist`**: adicionar
  `NSAudioCaptureUsageDescription` = "Record what your Mac is playing so it
  can be transcribed locally". É a permissão que o process tap usa (TCC
  "System Audio Recording").
- **Entitlements**: começar sem nada novo. Validar num build assinado com
  hardened runtime (spec R2). Se o tap falhar, testar
  `com.apple.security.device.audio-input`, que já existe.
- O tap do cpal tem `exclusive = true` e lista de processos vazia, então
  captura **todos** os processos, inclusive o próprio Handy. Como os sons de
  aviso não tocam nesse modo (§3.5), isso não é um problema.

---

## 7. Erros: tabela de ponta a ponta

| Situação                                     | Onde nasce           | `ErrorKind`                               | Notice                                           |
| -------------------------------------------- | -------------------- | ----------------------------------------- | ------------------------------------------------ |
| Nenhuma saída listada                        | `resolve_device`     | `DeviceUnavailable`                       | `NoOutputDevice`                                 |
| A saída escolhida sumiu                      | `resolve_device`     | (fallback para a padrão + setting zerado) | nenhum                                           |
| A saída sumiu durante a gravação             | watchdog / cpal      | `DeviceUnavailable`                       | `RecordingFailed` (o áudio já gravado é mantido) |
| macOS < 14.6 com setting antigo              | `open_device`        | `UnsupportedConfig`                       | `SystemAudioUnsupported`                         |
| O keepalive falhou e nada tocou              | watchdog             | `NoAudio` / `Stalled`                     | `RecordingFailed`                                |
| Endpoint em modo exclusivo (Windows)         | `build_input_stream` | `DeviceBusy`                              | `RecordingFailed`                                |
| Permissão de áudio do sistema negada (macOS) | (silêncio)           | —                                         | `SystemAudioSilent`                              |

---

## 8. Testes

**Crate:** §2.7.

**Handy, unitários** (`managers/audio.rs` `mod tests`):

- `recording_notice_code` com os dois kinds.
- `desired_device`: `SystemOutput` tem precedência sobre o clamshell, e o
  `selected_loopback_device` vazio vira `DefaultLoopback`.
- `CacheKey`: o mesmo nome em kinds diferentes não acerta o cache.

`resolve_device` chama `handy_recorder` direto. Para testar sem hardware,
extrair a função pura
`pick_device(desired, devices: &[InputDevice]) -> DeviceResolution` e testar
só ela. O `resolve_device` passa a ser enumeração + `pick_device`.

**Manual:** a matriz de aceite da spec §6, no Windows 11 (alto-falante
interno, fone USB, HDMI) e no macOS 14.6+ (alto-falantes internos, monitor
HDMI). No macOS < 14.6, só a opção desabilitada.

---

## 9. Decisões e alternativas descartadas

| Decisão                                       | Alternativa                      | Por que não                                                                           |
| --------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------- |
| Prefixo `loopback:` no id                     | campo `kind` em `RecorderConfig` | o UID do CoreAudio é o mesmo nas duas direções, e o Handy cacheia ids como string     |
| Keepalive silencioso                          | relaxar o watchdog para loopback | sem callbacks também não há readiness, e o overlay ficaria esperando o primeiro chunk |
| Kind do recorder aberto como fonte da verdade | ler `settings.input_source`      | a troca de setting no meio da gravação deixaria mute/sons/notices incoerentes         |
| `resolve_device` falha sem loopback           | cair no mic padrão               | gravaria o microfone sem o usuário saber                                              |
| Suprimir os sons de aviso nas chamadas        | dentro de `audio_feedback`       | `audio_feedback` não conhece a sessão, e o teste de som tem que continuar tocando     |
| Persistir nomes                               | persistir ids                    | o padrão do app é guardar nomes, e o id do WASAPI muda com o driver                   |
| Excluir no macOS os devices com entrada       | patch no `handy-cpal`            | mantém o cpal intocado na v1 (spec R1)                                                |

## 10. Questões abertas

1. O aggregate com tap do macOS entrega frames sem parar em silêncio? Isso
   decide se o macOS também precisa do keepalive (§2.5).
2. Precisa de entitlement extra em build assinado com hardened runtime (§6)?
3. ~~A `handy_recorder::Error` tem um construtor público?~~ Não tem
   (`Error::new` é `pub(crate)`). Resolvido com o `CaptureSourceError` do
   Handy (§3.3).
