# Explicit Sound Pack Activation Design

**Date:** 2026-10-28
**Status:** Approved for planning
**Related:** `docs/design/2026-10-28-optional-sound-packs-design.md`

## Problem

The optional-pack feature downloads a pack as soon as the cursor lands on it.
Arrow keys are for browsing, but today they silently start network requests and
switch the active sound. A user who scrolls past an optional pack pays for a
download they did not ask for, and the preview changes before they decide.

The app also conflates "the row I am looking at" with "the pack that is
playing". That makes startup fallback and config persistence ambiguous: if a
saved optional pack is not cached, the app must either download it at startup or
lie about which profile is selected.

## Goals

- Browsing with arrows is silent and side-effect free.
- A pack is downloaded only after an explicit activation.
- The active pack is visually distinct from the cursor.
- A saved optional profile survives startup fallback and volume changes.
- One download at a time; no queue, no cancel.

## Non-goals

- No new keybindings beyond `Enter`.
- No download queue, pause/resume, or cancellation.
- No change to pack formats, cache layout, or the download module.
- No change to volume, pause, preview, or quit semantics.

## Behavior

| Input / event | Result |
|---|---|
| `↑` / `↓` (`k` / `j`) | Move the cursor. No audio command, no download, no config write. |
| `Enter` on an embedded or cached pack | Load it, preview it, mark it active, save the profile. |
| `Enter` on an uncached optional pack | Start the download. The previous pack keeps playing. On success, switch, preview, mark active, save. |
| `Enter` while any download is running | Show `Download in progress: <name>` and do nothing else. |
| `Enter` on the active pack | Reload and preview it again, and save the same id. |
| `p` | Preview the active pack, not the cursor. |
| Startup, saved default or cached optional | Load it silently; mark it active. No preview. |
| Startup, saved optional not cached | Warn that it needs downloading; load the first embedded pack so the app is not silent. Cursor stays on the saved pack. No download. |
| Startup, saved profile missing | Warn; cursor and saved preference move to the first profile. |

Active and cursor may differ. This is intentional: browsing does not change what
is playing.

## UI model

```rust
pub struct App {
    pub profiles: Vec<ProfileEntry>,
    pub cursor: usize,
    pub active: Option<usize>,
    pub profile_id: String,
    pub volume: u8,
    pub paused: bool,
    pub listener_error: Option<String>,
    pub notice: Option<String>,
    pub quit: bool,
}
```

- `cursor` is the row the arrows move.
- `active` is the row whose pack is loaded in the audio thread. It is set only
  by `Notice::Activated`.
- `profile_id` is the saved preference. It is initialized from `config.profile`
  when that id exists, otherwise from the first profile id.
- `config()` returns `Config { profile: profile_id.clone(), volume }`.

`App::new` sets `cursor` to the saved profile's index, or `0` when it is
missing. It does not set `active`; the audio thread owns that.

## Audio model

### Commands and notices

- `AudioCmd::SetProfile(usize)` becomes `AudioCmd::Activate(usize)`.
- `Notice::Activated { index: usize, user: bool }` is sent after every
  successful load.
  - `user: true` means the load came from Enter or from a download the user
    started.
  - `user: false` means startup (including startup fallback).
- `Notice::Downloading`, `Downloaded`, `DownloadFailed` keep their current
  shapes and meanings.

### State

```rust
type Fetcher = fn(&Path, &str) -> Result<(), String>;

struct State {
    profiles: Vec<Profile>,
    packs_dir: Option<PathBuf>,
    notices: Sender<Notice>,
    tx: Sender<AudioCmd>,
    volume: u8,
    paused: bool,
    pack: Option<LoadedPack>,
    downloading: Option<String>,
    fetch: Fetcher,
}
```

`selected` and `pending` are removed. Downloads remain one at a time.
`fetch` is injectable so tests can exercise download starts without network;
production passes `download::fetch_and_install`.

### `start(saved)`

Called once from `spawn`, with no preview.

1. If `saved` is out of range, warn and fall back.
2. If the saved profile is optional and `download::is_installed` is false,
   warn `Saved profile '<name>' needs downloading; press Enter to download`
   and fall back. Never download here.
3. Otherwise load it. On success, store the pack and send
   `Activated { index: saved, user: false }`.
4. On load failure, warn and fall back.

Fallback loads the first embedded (`!is_optional()`) profile that decodes, then
index 0 if none did, stores it, and sends
`Activated { index: fallback, user: false }`. It does not change the saved
preference.

### `activate(index) -> bool`

The bool means "play a preview now".

1. Invalid index: warn, return false.
2. `downloading.is_some()`: warn `Download in progress: <name>`, return false.
   The UI normally catches this first; the audio guard is the authoritative
   backstop for races.
3. Optional and cached: load, store, send
   `Activated { index, user: true }`, return true.
4. Optional and cache is corrupt or missing:
   - If `is_installed` is true (files exist but do not decode), remove the cache
     directory and warn `Refreshing '<name>': <error>`.
   - Start the download, leave the old pack playing, return false.
5. Embedded or custom: load, store, send
   `Activated { index, user: true }`, return true. On load error, warn, leave
   the old pack, return false.

### `installed(index, ok) -> bool`

1. Clear `downloading` when its id matches the installed profile.
2. On failure, return false. The worker already sent `DownloadFailed`; the old
   pack stays.
3. On success, send `Downloaded(id)`, load the cached pack, store it, send
   `Activated { index, user: true }`, return true.
4. If the cached pack cannot be loaded, warn, keep the old pack, return false.

Downloads only start from `activate`, so a successful install is always a user
action. The pack activates even if the cursor moved during the download.

### `start_download(index, packs)`

Spawns one worker. The worker calls `self.fetch(packs, id)`:

- success: send `PackInstalled { index, ok: true }`
- failure: send `DownloadFailed { id, error }` then
  `PackInstalled { index, ok: false }`

`start_download` does not send `Activated`; only `installed` does.

### `spawn`

```rust
state.start(saved); // no preview
while let Ok(cmd) = rx.recv() {
    match cmd {
        AudioCmd::Activate(i) => {
            if state.activate(i) {
                state.preview(&stream);
            }
        }
        AudioCmd::PackInstalled { index, ok } => {
            if state.installed(index, ok) {
                state.preview(&stream);
            }
        }
        // Sound, Preview, SetVolume, SetPaused unchanged
    }
}
```

`Sound` still plays only when not paused. `Preview` still plays while paused.

## UI behavior

### `on_key`

- Up/Down (`k`/`j`): move `cursor` within bounds, return `None`. No notice, no
  config save, no audio command.
- Enter:
  - If `profiles` is empty, return `None`.
  - If any entry has `downloading == true`, set
    `notice = Some("Download in progress: <name>")` using that entry's name,
    and return `None`.
  - Otherwise return `Some(AudioCmd::Activate(cursor))`. Do not change
    `active` or `profile_id`; wait for `Activated`.
- `p`: return `Some(AudioCmd::Preview)`. The audio thread previews the loaded
  pack, which is the active one.
- Volume, pause, quit: unchanged.

### `on_notice` returns `bool`

The bool means "save config now".

- `Activated { index, user }`:
  - Set `active = Some(index)` when the index is valid.
  - If `user`, set `profile_id = profiles[index].id`, set
    `notice = Some(format!("Loaded {name}"))`, return true.
  - If not `user`, leave `profile_id` and the current notice untouched, return
    false. This keeps the startup fallback warning visible.
- `Warn(e)`: set `notice = Some(e)`, return false. Do not change `active` or
  `profile_id`.
- `Downloading(id)`: set that entry's `downloading = true`; notice
  `Downloading <name>…`; return false.
- `Downloaded(id)`: set `available = true`, `downloading = false`; notice
  `Downloaded <name>`; return false.
- `DownloadFailed { id, error }`: set `downloading = false`; notice
  `Could not download <name>: <error>`; return false.
- `ListenerFailed`: unchanged; return false.

`run` saves config when `on_notice` returns true, and on volume changes. A save
failure becomes a notice, never a crash.

### Rendering

Each row is:

```text
<cursor><active> <name><suffix>
```

- `<cursor>` is `▸` on the cursor row, otherwise a space.
- `<active>` is `♪` on the active row, otherwise a space.
- `<suffix>` is `  …` while downloading, `  ↓` when an optional is not
  available, otherwise empty.
- Cursor row: cyan, bold.
- Active row that is not the cursor: green, bold.
- Downloading row that is neither cursor nor active: yellow.
- Unavailable optional row: dark gray.
- All others: default.

The list uses `ListState` with `cursor`, so the cursor stays visible with 18
entries.

Footer:

```text
↑↓ browse  enter load  ←→ volume  space pause  p preview  q quit
```

## Config persistence

- Saved on `Activated { user: true }` and on volume changes.
- Never saved on arrow movement, startup activation, warnings, download
  progress, or failures.
- Volume changes use `profile_id`, so a startup fallback does not replace a
  saved optional id with the temporary fallback id.
- If the saved profile disappears from the catalog, `App::new` warns, moves
  `cursor` and `profile_id` to the first profile, and the next user activation
  saves that.

## Edge cases

| Case | Expected |
|---|---|
| Saved optional, uncached, startup | Fallback pack active, cursor on saved, warning, no network. Enter downloads. |
| Saved optional, cached, startup | Loaded and active, no preview, no network. |
| Saved optional, corrupt cache, startup | Warning, fallback active, no network. Enter removes the corrupt cache and downloads. |
| Enter during a download | Notice `Download in progress: <name>`; no queued activation; old pack stays. |
| Download fails | Old pack stays active; entry still unavailable; `DownloadFailed` notice. |
| Arrows during a download | Cursor moves; the pack still activates when the download succeeds. |
| Enter on active pack | Reloads, previews, saves the same id. |
| Config save fails | Notice; audio and UI keep working. |
| No profiles | Enter and arrows are no-ops. |

## Testing

### UI

- Arrows move the cursor, return no command, and do not change `active`.
- Enter returns `Activate(cursor)`.
- Enter is rejected with `Download in progress: …` when an entry is
  downloading.
- `Activated { user: true }` sets active/profile_id, sets a loaded notice, and
  returns true.
- `Activated { user: false }` sets active only and returns false, leaving the
  startup warning intact.
- `Warn` does not change active or profile_id.
- Rendering shows `▸` on the cursor, `♪` on the active pack, and both when they
  coincide.
- Volume save after startup fallback keeps the saved optional id.
- Existing UI tests updated for `cursor`/`active`.

### Audio

- `start` with a saved default loads and sends `Activated { user: false }`.
- `start` with an uncached optional falls back without downloading.
- `start` with a cached optional loads it.
- `start` with a corrupt optional cache falls back without downloading.
- `activate` on embedded/cached loads and sends `Activated { user: true }`.
- `activate` on an uncached optional sets `downloading` and sends no
  `Activated` (stub fetcher).
- `activate` on a corrupt cache removes the directory and sets `downloading`.
- `activate` while downloading is rejected with a warning.
- `installed` success loads the pack and sends `Activated { user: true }`.
- `installed` failure keeps the old pack and sends no `Activated`.
- Existing 60 tests updated and passing.

## Acceptance criteria

1. Moving with arrows never downloads, plays, or writes config.
2. Enter on an uncached optional is the only way to start its download.
3. The pack that is playing carries `♪`; the cursor row carries `▸`.
4. Startup with an uncached saved optional shows the fallback and a warning, and
   makes no network request.
5. Enter during a download is rejected with the name of the running download.
6. A failed download leaves the old pack playing and the profile unchanged.
7. A saved optional survives startup fallback and volume changes.
8. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
   `cargo test` pass.

## Alternatives considered

- **UI-only gate (B):** keep `SetProfile` and assume the UI's selected row is
  playing. Rejected because startup fallback and download completion make the
  UI's assumption false, and config would be written optimistically.
- **Separate Download and Activate commands (C):** UI orchestrates download
  then activation. Rejected because it duplicates audio state and races with
  the worker.
- **UI `pending_activation` flag:** rejected in favor of
  `Activated { user }`. The flag needs special cases for warnings and rejected
  Enters; the flag also risks a rejected activation clearing the pending state
  of a download that is still finishing.

## Risks

- `ureq` blocks one worker thread; this is unchanged and bounded by one
  download.
- The UI's `downloading` flag can lag the audio thread by one poll interval.
  The audio `activate` guard is the backstop; a rare race may show two notices
  but cannot start a second download.
- Cursor/active divergence can surprise users at first; the `▸`/`♪` markers and
  the footer's `enter load` make the model explicit.

## Files

- `src/audio.rs`: command/notice changes, `State::start`, `activate`,
  `installed`, fetcher injection, tests.
- `src/ui.rs`: `App` fields, `on_key`, `on_notice` bool, rendering, tests.
- `src/main.rs`: `Notice::Activated { index, user }`; initial cursor/profile id
  wiring.
- `README.md`: keys table, footer, startup fallback.
- `docs/plans/2026-10-28-explicit-pack-activation.md`: implementation plan.
