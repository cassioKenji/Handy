# Handoff: captura do áudio de saída do sistema, implementação no Windows

Escrito em 2026-11-24 para o modelo que vai implementar esta feature numa
máquina **Windows**. Leia este arquivo inteiro antes de qualquer outra coisa.

---

## 1. Em uma frase

Este repo é um **fork do Handy** (app de ditado: Tauri 2 + Rust + React). O
usuário quer poder escolher como **fonte de entrada a saída de uma placa de
som** (o que está tocando nos alto-falantes ou fones), além do microfone,
**no Windows e no macOS**. A análise, a spec, o design e as tasks estão
prontos. **Nenhum código foi alterado ainda.** Você começa pela
implementação.

## 2. Sobre o usuário

- Escreve em **português (Brasil)**. Responda em português. O código, os
  comentários e as mensagens de commit ficam em **inglês** (padrão do repo).
- Gosta de ver uma análise antes de mexer em código, e decide por etapas.
  Quando houver uma decisão que é dele (escopo, UX, algo que se afaste do
  design), **pergunte**, não assuma.
- Decisões que ele já tomou (não pergunte de novo):
  1. Plataformas: **Windows e macOS**. Linux fica fora da v1.
  2. UI: um **seletor "Fonte de entrada"** (Microfone / Saída do sistema),
     não uma lista misturada.
  3. Microfone e saída **ao mesmo tempo** (mixados): **fica para depois** (v2).
  4. A crate `handy-recorder` vai ser **copiada para dentro do repo**
     (`crates/handy-recorder`), não um fork em outro repositório.

## 3. Documentos (nesta pasta)

| Arquivo | O que é | Quando usar |
|---|---|---|
| [spec.md](spec.md) | **O quê**: comportamento, escopo e critérios de aceite (§6) | Para saber se está pronto |
| [design.md](design.md) | **Como**: arquitetura, tipos, fluxos e decisões com o porquê (§9) | Para entender o porquê de cada task |
| [tasks.md](tasks.md) | **Passo a passo**: 18 tasks com código, verificação e commit | **O que você executa** |
| handoff.md | Este arquivo: contexto, ambiente Windows e armadilhas | Primeiro |

**Precedência** em caso de conflito: `handoff.md` > `tasks.md` > `design.md` >
`spec.md`. Os documentos mais novos foram escritos com mais informação do
código real. Anote no PR todo desvio que você fizer de qualquer um deles.

**Estado no git:** a pasta `docs/specs/` **ainda não está commitada** (está
untracked na `main`). Veja o §6, passo 1.

## 4. Onde está a mudança (o mapa mental)

```
UI (Settings › Sound)  →  settingsStore.ts  →  commands/audio.rs
  →  managers/audio.rs (AudioRecordingManager)
    →  crates/handy-recorder   ← a maior parte da mudança
      →  handy-cpal 0.18.2     ← NÃO mexer: ele já faz loopback
```

- O **cpal** (fork `handy-cpal`) já sabe gravar uma saída: no Windows, chamar
  `build_input_stream` num device de saída liga o
  `AUDCLNT_STREAMFLAGS_LOOPBACK` sozinho. No macOS 14.6+, ele cria um
  process tap.
- A **`handy-recorder`** é quem esconde isso: ela só lista
  `host.input_devices()` e abre tudo com `default_input_config()`, que **dá
  erro num device de saída no Windows**. As tasks T2 a T8 resolvem isso.
- No **Handy**, os pontos sensíveis: o "Mute while recording" mutaria
  justamente o que está sendo gravado, os sons de início e fim iriam parar
  na transcrição, e a resolução do device pelo **nome** confundiria mic e
  saída de mesmo nome ("Realtek Audio").

## 5. Ambiente Windows

### 5.1 O repo

- Este é o mesmo repositório que estava aberto no WSL
  (`/mnt/d/Dev/godot_projects/Handy` = `D:\Dev\godot_projects\Handy`).
- Use o **git do Windows**. Se o primeiro `git status` mostrar centenas de
  arquivos modificados que ninguém tocou, a causa é permissão de arquivo ou
  fim de linha:
  ```powershell
  git config core.filemode false
  git config core.autocrlf false
  git status
  ```
  Depois disso só deve sobrar `?? docs/specs/`. Se ainda houver arquivos
  modificados, **não faça commit deles**: pare e mostre ao usuário.

### 5.2 Pré-requisitos (do `BUILD.md`, seção Windows)

Confira cada um com o comando ao lado. Não instale nada sem avisar o usuário.

| Ferramenta | Verificar | Instalar |
|---|---|---|
| Rust (stable, MSVC) | `rustc -V`, `cargo -V` | https://rustup.rs |
| Bun | `bun -v` | https://bun.sh |
| VS Build Tools 2022 com "Desktop development with C++" | `where cl` num "Developer PowerShell" | instalador do Visual Studio |
| CMake no `PATH` | `cmake --version` | `winget install Kitware.CMake` |
| Vulkan SDK | `echo $env:VULKAN_SDK` (não vazio) | `winget install KhronosGroup.VulkanSDK`, depois **abra um terminal novo** |

### 5.3 Baseline: confirme que o Handy original compila e roda

**Antes da T1.** Se der problema aqui, é o ambiente, não a feature:

```powershell
bun install
New-Item -ItemType Directory -Force src-tauri\resources\models | Out-Null
curl.exe -L -o src-tauri\resources\models\silero-vad-v6.2-F32.gguf `
  https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
bun run tauri dev
```

O app tem que abrir, baixar ou selecionar um modelo e transcrever do
microfone. O primeiro build é longo (transcribe-cpp + Vulkan). Se travar com
erro de caminho longo (`MSB3491`, `FTK1011`, `MSB6003`), veja a seção de
Troubleshooting do `BUILD.md`.

Guarde o tempo e o resultado do baseline e informe ao usuário.

### 5.4 Comandos das tasks, em PowerShell

As tasks foram escritas com comandos estilo Unix. No PowerShell:

| tasks.md | PowerShell |
|---|---|
| `cd src-tauri && cargo test` | `cd src-tauri; cargo test; cd ..` (ou `cargo test --manifest-path src-tauri/Cargo.toml`) |
| `grep -rn "X" pasta` | `Select-String -Path pasta\* -Pattern "X" -Recurse` ou `git grep -n "X" -- pasta` (**prefira `git grep`**) |
| `grep … \| wc -l` | `(git grep -l "X" -- src/i18n/locales).Count` |
| `/tmp/…` | `$env:TEMP\…` |
| `bun /tmp/add-locale-keys.ts` | `bun $env:TEMP\add-locale-keys.ts` (rodado da raiz do repo) |

O `cargo` da crate vendorizada:
`cargo test --manifest-path crates/handy-recorder/Cargo.toml --features test-backend`.

### 5.5 T1 em PowerShell (substitui os passos 1 a 4 da T1)

```powershell
$work = Join-Path $env:TEMP "hr"
New-Item -ItemType Directory -Force $work | Out-Null
curl.exe -L -o "$work\handy-recorder-0.1.1.crate" `
  https://crates.io/api/v1/crates/handy-recorder/0.1.1/download
(Get-FileHash "$work\handy-recorder-0.1.1.crate" -Algorithm SHA256).Hash
# Tem que ser (o PowerShell mostra em maiúsculas):
# 29602F3465CB080CC437B08903EA7EF395C55A41120264A897ACDD25FF9B255A
tar -xzf "$work\handy-recorder-0.1.1.crate" -C $work
New-Item -ItemType Directory -Force crates | Out-Null
Copy-Item -Recurse "$work\handy-recorder-0.1.1" crates\handy-recorder
Remove-Item -ErrorAction SilentlyContinue crates\handy-recorder\.cargo_vcs_info.json
```

**Se o hash não bater, pare e avise o usuário.** O resto da T1 (passos 5 a 7)
continua igual.

## 6. Por onde começar

1. **Branch.** Não trabalhe na `main`. Pergunte ao usuário se pode criar
   `feat/system-audio-capture`. Com o ok:
   ```powershell
   git switch -c feat/system-audio-capture
   git add docs/specs/system-audio-capture
   git commit -m "docs: spec, design and tasks for system audio capture"
   ```
   (Termine toda mensagem de commit com a linha de co-autoria que a sua
   ferramenta indicar.)
2. **Commits por task.** As tasks trazem uma mensagem de commit cada. Na
   primeira vez, **confirme com o usuário** que ele quer um commit por task.
   Se ele preferir outra granularidade, siga a dele.
3. **Baseline** (§5.3).
4. **T1 a T18, em ordem** ([tasks.md](tasks.md)). Uma task por vez. Rode a
   verificação de cada uma antes de seguir. Ao fim de cada **fase** (A:
   crate, B: backend, C: frontend, D: final), dê ao usuário um resumo
   curto: o que mudou, o que foi verificado e o que falta.
5. **Não faça push nem abra PR** sem o usuário pedir. Quando ele pedir, siga
   a T18 §3 e o `.github/PULL_REQUEST_TEMPLATE.md` à risca (regra do
   `AGENTS.md`).

## 7. Armadilhas conhecidas

1. **O `bindings.ts` é gerado** pelo `bun run tauri dev` em debug (T14). No
   Windows você consegue rodar o app, então **regenere** em vez de editar à
   mão.
2. **As traduções são 29 locales.** O `bun run check:translations` falha se
   faltar uma chave em qualquer um. Use o script do Apêndice A das tasks e
   depois `bun run format:frontend`. Confira no `git diff` que nenhum JSON
   perdeu linhas.
3. **O keepalive do Windows (T6) é essencial.** Sem ele, gravar com nada
   tocando derruba o recorder em 5 a 10 s (`NoAudio`/`Stalled` do watchdog).
   O teste é o critério 2 da T18. Se ele falhar, procure `keepalive` no log
   (`--debug`).
4. **Os ids de loopback começam com `loopback:`** (ex.:
   `loopback:wasapi:{0.0.0.00000000}.{…}`). Nunca passe um id de loopback
   para `list_input_devices`/mic, nem o contrário.
5. **`device: None` no `RecorderConfig` abre o MICROFONE padrão.** No modo
   saída, o Handy **sempre** resolve um id explícito (`pick_device`, T10). Se
   não houver nenhuma saída, a gravação **falha** com `NoOutputDevice`, e
   nunca cai no mic.
6. **`InputDevice` e `DeviceKind` são `#[non_exhaustive]`.** Fora da crate não
   dá para construí-los nem dar `match` sem um braço `_`. Por isso o Handy usa
   o próprio `InputSource` e o `DeviceEntry` (T10).
7. **O `handy_recorder::Error::new` é `pub(crate)`.** O Handy não consegue
   criar erros da crate. Use o `CaptureSourceError` (T10/T13).
8. **Os números de linha nas tasks são aproximados.** Ache o código pelo
   trecho de texto citado.
9. **O macOS não é testável no Windows.** Faça o código macOS compilar
   logicamente (os `cfg` certos, revisados a olho) e marque na tabela da T18
   os critérios 4 e 5 como "não testado: precisa de Mac". **Não** finja ter
   testado.
10. **Testes de hardware da crate** (`#[ignore]`):
    `cargo test --manifest-path crates/handy-recorder/Cargo.toml --features test-backend -- --ignored --nocapture lists_and_resolves_real_devices`.
    Cole a saída no resumo da fase A. Ela mostra as saídas da máquina do
    usuário.

## 8. Quando parar e perguntar ao usuário

- O hash da crate não bate (T1).
- O baseline (§5.3) não compila ou não roda.
- Uma task exige mexer num arquivo que ela não lista.
- Um teste existente (não um seu) quebra e o conserto não é óbvio.
- Um critério de aceite da T18 falha depois de uma tentativa razoável de
  conserto.
- Alguma decisão de UX ou escopo que não esteja nos documentos.

## 9. Questões em aberto (do design §10)

1. O macOS entrega frames sem parar quando nada toca? Isso decide se o macOS
   também precisa do keepalive. **Só dá para responder num Mac.** Deixe como
   está e anote.
2. O build assinado do macOS precisa de entitlement extra? Também só num Mac.
3. ~~Construtor público de erro da crate~~: resolvido (`CaptureSourceError`).

## 10. O que não está disponível para você

- O código da `handy-recorder` e do `handy-cpal` foi lido no WSL, em uma
  pasta temporária que **não existe** no Windows. Para consultar o cpal
  (por exemplo, `host/wasapi/device.rs` ou `host/coreaudio/macos/loopback.rs`),
  use o fonte que o cargo baixa depois do primeiro build:
  `%USERPROFILE%\.cargo\registry\src\index.crates.io-*\handy-cpal-0.18.2\`.
- O histórico desta conversa. Tudo o que importa está nestes quatro
  arquivos.
