# Spec: captura do áudio de saída do sistema (loopback)

Status: rascunho · 2026-11-24 · Plataformas: Windows e macOS

## 1. Objetivo

Hoje o Handy só grava de microfones. Este fork permite escolher, como fonte de
entrada da dictation, **a saída de qualquer placa de som** (o que está tocando
nos alto-falantes ou fones). Assim dá para transcrever um vídeo, uma chamada ou
um podcast.

### Fora do escopo (v1)

- Microfone e saída ao mesmo tempo (mixados). Fica para a v2; ver §9.
- Linux. O backend PulseAudio já lista os "Monitor of …". A v1 não mexe no
  filtro `!d.is_monitor` em `commands/audio.rs`, mas o desenho (§4) deixa
  isso fácil de ligar depois.
- Capturar só o áudio de um aplicativo.
- Mudanças na VAD, nos modelos ou no pipeline de transcrição. O recorder
  continua entregando 16 kHz mono em blocos do tamanho da VAD.

## 2. Comportamento para o usuário

1. Em **Settings → Sound** aparece um seletor **Fonte de entrada** com duas
   opções: **Microfone** (padrão, o comportamento de hoje) e **Saída do
   sistema**.
2. Com **Microfone**, nada muda.
3. Com **Saída do sistema**:
   - O dropdown de microfone dá lugar ao **Dispositivo de saída a gravar**,
     com "Padrão" (a saída padrão do sistema) e cada saída listada pelo SO.
   - Ficam ocultos: Input Channel, o microfone de clamshell e "Mute while
     recording" (ver §5.4).
   - O atalho, o push-to-talk, o overlay, a VAD, a transcrição e a colagem
     funcionam igual. Só muda de onde vem o áudio.
4. Trocar de fonte preserva a escolha da outra. Voltar para Microfone recupera
   o microfone que estava selecionado.
5. No macOS abaixo de 14.6, a opção **Saída do sistema** aparece desabilitada,
   com uma explicação.
6. Se a gravação terminar e nada estiver tocando, aparece o aviso
   "Nothing Was Playing" no lugar de "Microphone Silent".

## 3. Onde está o código

```
src/components/settings/  (InputSourceSelector, SystemOutputSelector: novos)
  → src/stores/settingsStore.ts
  → src-tauri/src/commands/audio.rs
  → src-tauri/src/managers/audio.rs  (AudioRecordingManager)
  → crates/handy-recorder            (vendorizada, modificada)
  → handy-cpal 0.18.2                (sem mudanças: já faz o loopback)
```

O loopback já existe no `handy-cpal`. Basta chamar `build_input_stream` num
device de **saída**:

- **Windows (WASAPI):** liga `AUDCLNT_STREAMFLAGS_LOOPBACK` quando o device é
  `eRender` (`host/wasapi/device.rs`).
- **macOS (CoreAudio):** quando o device não tem entrada (`!supports_input()`),
  cria um process tap (`CATapDescription`, `Unmuted`) e um aggregate device
  privado (`host/coreaudio/macos/loopback.rs`). Pede **macOS 14.6+**.

## 4. Mudanças na `handy-recorder`

### 4.1 Vendorizar

- Copiar `handy-recorder` 0.1.1 do crates.io (mesmo checksum do `Cargo.lock`)
  para `crates/handy-recorder/`.
- Em `src-tauri/Cargo.toml`, trocar a dependência:
  `handy-recorder = { path = "../crates/handy-recorder" }`.
- Primeiro commit: só a cópia, sem nenhuma alteração. O diff do fork fica
  legível e o merge com versões futuras fica mais fácil.

### 4.2 API pública

```rust
// device.rs
#[non_exhaustive]
pub enum DeviceKind {
    Microphone,
    /// A saída de uma placa, gravada por loopback (WASAPI / CoreAudio tap).
    Loopback,
}

pub struct InputDevice {
    // ... campos atuais ...
    pub kind: DeviceKind,
}

/// Só microfones. Mesmo comportamento de hoje (compatível).
pub fn list_input_devices() -> Result<Vec<InputDevice>, Error>;

/// Saídas que dá para gravar por loopback. Vazio onde não há suporte
/// (macOS < 14.6, Linux na v1).
pub fn list_loopback_devices() -> Result<Vec<InputDevice>, Error>;

/// Se a plataforma grava a saída do sistema (versão do SO).
pub fn loopback_supported() -> bool;
```

**IDs:** o id de um device de loopback tem o prefixo `loopback:`
(`loopback:<cpal DeviceId>`). No CoreAudio, a entrada e a saída de uma mesma
placa podem ter o **mesmo UID**, e o prefixo tira essa ambiguidade. O
`RecorderConfig::device` continua sendo `Option<String>`, e o backend lê o
prefixo.

### 4.3 `backend/cpal/mod.rs`

- **`enumerate_loopback(host)`**, nova: percorre `host.output_devices()`,
  monta `InputDevice { kind: Loopback, id: "loopback:…", is_default: <saída padrão>,
channels: default_output_config().channels() }`, com nomes e `occurrence`
  como no `enumerate` atual.
  - **macOS:** **excluir saídas cujo device também tenha entrada**
    (`default_input_config().is_ok()`, por exemplo interfaces USB e headsets
    que o CoreAudio vê como um device só). Nelas o cpal grava o **microfone**
    em vez do loopback (`supports_input()` em `device.rs:728`). Os alto-falantes
    internos e o HDMI aparecem como devices só de saída e funcionam. Ver §8 R1.
- **`open_device(id)`**: com o prefixo `loopback:`, procura em
  `enumerate_loopback` e usa **`default_output_config()`** no lugar de
  `default_input_config()`. No Windows o `default_input_config` de um device
  `eRender` retorna `UnsupportedOperation`.
- **`CpalOpenDevice::start`** para loopback:
  - **Windows:** abrir também um **stream de saída silencioso no mesmo
    device**, generalizando o `silent_output()` que hoje é só do macOS. O
    WASAPI loopback **não manda pacotes quando nada toca**, e sem pacotes o
    watchdog derruba o recorder com `NoAudio` (10 s) ou `Stalled` (5 s). Com
    o render silencioso ativo, o loopback entrega zeros. Esse stream fica num
    campo novo de `CpalStream` (ex.: `keepalive: Option<cpal::Stream>`), e o
    `Drop` o libera.
  - O buffer de 500 ms do WASAPI continua igual.
  - **macOS:** sem keepalive na v1, desde que o aggregate entregue frames
    continuamente (verificar; ver §8 R3).

### 4.4 Permissões (`capture/engine.rs`)

- Em `open` (`engine.rs:631`) e no watchdog (`engine.rs:381`), a verificação
  `denial_is_silent() && permission_status() == Denied` olha a permissão de
  **microfone**. Ela não vale para loopback: pular quando
  `info.kind == Loopback`.
- O macOS não oferece API pública para ler a permissão "System Audio
  Recording". Quando ela é negada, o tap entrega silêncio, e quem cobre esse
  caso é a detecção de silêncio do Handy (§5.5).

### 4.5 Testes

- `backend/fake.rs`: aceitar devices `Loopback` e ids com o prefixo.
- Testes novos em `tests/`: `list_loopback_devices` separado de
  `list_input_devices`; open com o prefixo; o open pula a checagem de
  permissão de microfone quando o device é de loopback.
- Teste manual com hardware, no padrão `lists_and_resolves_real_devices`
  (`#[ignore]`), cobrindo os loopbacks.

## 5. Mudanças no Handy

### 5.1 Settings (`src-tauri/src/settings.rs`)

```rust
#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputSource { #[default] Microphone, SystemOutput }

#[serde(default)] pub input_source: InputSource,
/// Nome da saída a gravar; None = saída padrão do sistema.
#[serde(default)] pub selected_loopback_device: Option<String>,
```

- Os defaults mantêm os settings existentes compatíveis, sem migração.
- `selected_microphone`, `selected_channel` e `clamshell_microphone` não mudam.

### 5.2 Comandos (`src-tauri/src/commands/audio.rs`)

| Comando                                         | Faz                                                                                                              |
| ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `get_input_source` / `set_input_source(source)` | Persiste e chama `rm.update_selected_device()` em `spawn_blocking`, no mesmo padrão de `set_selected_microphone` |
| `get_available_loopback_devices()`              | `handy_recorder::list_loopback_devices()`, com "Default" no topo e a mesma forma de `get_available_microphones`  |
| `get/set_selected_loopback_device(name)`        | Igual ao par do microfone                                                                                        |
| `is_loopback_supported()`                       | `handy_recorder::loopback_supported()`                                                                           |

Registrar tudo em `lib.rs` (`collect_commands!`) e regenerar `bindings.ts`
rodando o app em debug.

### 5.3 Resolução do device (`src-tauri/src/managers/audio.rs`)

- `DesiredMicrophone` ganha a variante `Loopback(Option<String>)`. O
  `desired_microphone()` devolve essa variante quando
  `input_source == SystemOutput`, **antes** da checagem de clamshell.
- `resolve_microphone_device()`:
  - Com `Loopback(None)`: resolve a **saída padrão** pelo `is_default` de
    `list_loopback_devices()`. O `device: None` do recorder abriria o
    **microfone** padrão, então não pode ser usado aqui. Esse resultado não
    vai para o cache: a saída padrão muda.
  - Com `Loopback(Some(name))`: procura pelo nome **só** em
    `list_loopback_devices()`. Isso evita a colisão com um microfone de mesmo
    nome (ex.: "Realtek Audio").
  - Cache: a chave de `cached_device` passa a incluir o tipo (ex.:
    `("loopback", name)`), para que um id de microfone não seja reaproveitado
    para uma saída com o mesmo nome.
  - Saída selecionada que sumiu: cair na saída padrão e persistir
    `selected_loopback_device = None`, no mesmo padrão de
    `persist_default_microphone_after_fallback` (emitindo `settings-changed`).
- `microphone_config()`: em loopback, `channels = MixToMono` e
  `take_headset = false`. `selected_channel` e o modo AirPods são ignorados.

### 5.4 Mute e sons de aviso

- **`apply_mute()`**: não fazer nada quando `input_source == SystemOutput`. O
  mute do sistema silenciaria justamente o que está sendo gravado.
- **Sons de início e fim** (`audio_feedback.rs`): **não tocar** em modo
  SystemOutput na v1. Se tocassem, entrariam na gravação e virariam texto. A
  alternativa (só suprimir quando a saída dos avisos for a mesma que está
  sendo gravada) fica para depois.

### 5.5 Silêncio e avisos (`notices`)

- Novo `NoticeCode::SystemAudioSilent` (nível Warning, fonte Dictation),
  seguindo o passo a passo de "Error notices" no AGENTS.md:
  `notices.rs` (level + `tray_title`), `errors.systemAudioSilentTitle` em
  `en/translation.json` e em todos os locales (`bun run check:translations`)
  e `NOTICE_DISPLAY` em `src/lib/notices.ts`.
  - Título: "Nothing Was Playing" (cabe nos 215px do overlay).
  - Explicação: no macOS, citar a permissão "System Audio Recording" em
    Privacy & Security.
- `finish_without_text()` em `actions.rs`: quando `input_silent` e a fonte é
  SystemOutput, emitir `SystemAudioSilent` em vez de `MicrophoneSilent`.
- `recording_notice_code()`: em loopback, `DeviceUnavailable` vira um novo
  `NoOutputDevice` (Error), ou reaproveita `RecordingFailed` na v1. Decidir na
  implementação; a recomendação é o código novo.

### 5.6 Frontend

- **`InputSourceSelector.tsx`** (novo): segmented control ou dropdown
  Microfone / Saída do sistema, desabilitado com tooltip quando
  `isLoopbackSupported()` é falso.
- **`SystemOutputSelector.tsx`** (novo): igual ao `MicrophoneDropdown`, mas
  com `loopbackDevices` e `selected_loopback_device`.
- Na seção Sound, renderização condicional: em SystemOutput, ocultar
  `MicrophoneSelector`, `ChannelSelector`, `ClamshellMicrophoneSelector` e
  `MuteWhileRecording`.
- `settingsStore.ts`: updaters para `input_source` e
  `selected_loopback_device`, mais o estado `loopbackDevices` com
  `refreshLoopbackDevices`, e tratar `settings-changed` para
  `selected_loopback_device`.
- O onboarding e o "Try it" continuam só com microfone.
- i18n: chaves novas em `settings.sound.inputSource.*` e
  `settings.sound.systemOutput.*`, em todos os locales.

### 5.7 macOS: bundle

- `src-tauri/Info.plist`: adicionar `NSAudioCaptureUsageDescription`. Sem
  essa chave, o process tap entrega silêncio ou nem pede a permissão.
- `Entitlements.plist`: verificar se o tap exige algo além de
  `audio-input` (ver §8 R2).
- `tauri.conf.json` mantém `minimumSystemVersion: 10.15`. A versão é checada
  em runtime por `loopback_supported()`.

## 6. Critérios de aceite

1. **Windows:** com Saída do sistema e "Padrão", tocar um vídeo de fala no
   navegador e usar o atalho cola a transcrição.
2. **Windows:** gravar com nada tocando por mais de 15 s não derruba o recorder
   (sem `NoAudio`/`Stalled`) e termina com o aviso "Nothing Was Playing".
3. **Windows:** escolher uma saída específica (ex.: um fone USB) grava só o que
   toca nela.
4. **macOS 14.6+:** como 1, nos alto-falantes internos. No primeiro uso o
   sistema pede a permissão "System Audio Recording".
5. **macOS < 14.6:** a opção aparece desabilitada, com explicação.
6. Com "Mute while recording" ligado e em modo SystemOutput, o áudio não é
   mutado e a transcrição sai.
7. Os sons de aviso não entram na transcrição em modo SystemOutput.
8. Voltar para Microfone restaura o microfone anterior, e a gravação funciona
   como antes.
9. Um microfone e uma saída com o mesmo nome não se confundem.
10. Com o modo always-on e a fonte SystemOutput, o stream fica aberto, e
    trocar de fonte reabre sem reiniciar o app.
11. Passam `bun run lint`, `bun run format:check`, `bun run check:translations`
    e `cargo test` (incluindo `crates/handy-recorder`).

## 7. Ordem de implementação

1. Vendorizar `handy-recorder`, sem alterações (commit isolado).
2. Recorder: `DeviceKind`, `list_loopback_devices`, ids com prefixo e
   `open_device` com a config de saída. Testes com o fake backend.
3. Recorder: keepalive silencioso no Windows e a checagem de permissão
   ignorada em loopback.
4. Handy backend: settings, resolução do device, comandos e bindings.
5. Handy: mute, sons de aviso e o notice `SystemAudioSilent`.
6. Frontend: os seletores, o store e o i18n.
7. macOS: Info.plist e o filtro de devices com entrada. Testar num Mac real.

## 8. Riscos e pontos a verificar

| #   | Risco                                                                                       | Mitigação                                                                           |
| --- | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| R1  | macOS: saídas de devices que também têm entrada (interfaces USB, headsets) gravariam o mic  | Excluir da lista na v1. Na v2, patch no `handy-cpal` para forçar o `LoopbackDevice` |
| R2  | macOS: permissão de áudio do sistema sem API de status, e entitlements extras desconhecidos | Testar num build assinado. A detecção de silêncio vira o aviso §5.5                 |
| R3  | macOS: o aggregate com tap pode parar de mandar frames em silêncio                          | Testar o critério 2 no Mac. Se parar, usar o mesmo keepalive do Windows             |
| R4  | Windows: a saída padrão muda durante uma gravação                                           | A v1 continua no device resolvido no open. A troca vale na próxima gravação         |
| R5  | Windows: loopback de devices em modo exclusivo (outros apps)                                | Erro de open → notice. Fica documentado como limitação                              |
| R6  | Merge com versões novas da `handy-recorder`                                                 | Commit de cópia isolado, e as mudanças concentradas em `backend/cpal` e `device.rs` |

## 9. v2 (anotado, não implementar)

- Microfone + saída mixados: dois `Recorder`s num sink que soma os dois com
  alinhamento por timestamp, ou um backend "composite" dentro da
  `handy-recorder`.
- Linux: liberar os monitor sources como `DeviceKind::Loopback`.
- Sons de aviso inteligentes: suprimir só quando tocarem na saída gravada.
