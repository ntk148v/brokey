# Optional sound packs downloaded on demand

Date: 2026-10-28
Status: Draft for review

## Problem

Brokey embeds 11 full sound packs in the binary. Every profile is compiled in
with `include_bytes!` even though a user only ever uses one at a time. That
costs roughly 3.1 MB of assets and caps the catalog at what fits in the
repository.

MechVibes/MechSim publish many more real mechanical-switch recordings. Users
should be able to pick them from the same profile list, but only download the
ones they choose. The embedded set should shrink to a small offline-ready
starter set.

The current selection code already loads only the selected pack lazily; it does
not decode all packs. The change needed is to make uncached profiles
downloadable on selection and cached on disk.

## Goals

- Keep four profiles embedded and fully offline: Cherry MX Blue, Red, Brown,
  and Topre.
- Offer the other 14 MIT-licensed packs in the same profile list, marked as
  optional.
- Download an optional pack the first time it is selected, cache it under the
  user config directory, and load it from cache thereafter.
- Keep the old pack playing while a download is in progress; a failed download
  must never silence the app.
- Avoid downloading packs the user merely scrolls past.
- Keep ids and `config.toml` compatible with existing installs.
- Keep the repository, build, and runtime lightweight; no build-time network
  access.

## Non-goals

- Downloading the non-mechanical or license-unclear packs on mechvibes.com.
- Runtime conversion of MechVibes OGG/MP3 sprite packs. Optional packs are
  prebuilt brokey FLAC packs, exactly like the embedded ones.
- Automatic pack updates after the first download.
- A download manager UI, progress bars, or new keybindings.
- Changing the custom-pack format in `<config>/brokey/sounds/`.

## Catalog

Default profiles are compiled into the binary from `assets/sounds/<id>/`.
Optional profiles are metadata only; their audio lives in
`assets/sounds-optional/<id>/` on the `master` branch of the GitHub repository
and is fetched on demand.

Display names change for MX switches to state the keycap material. Profile ids
do not change, so saved configs keep working.

### Embedded (default)

| id | Display name | MechSim source dir |
|---|---|---|
| `mx-blue` | Cherry MX Blue (ABS) | `cherrymx-blue-abs` |
| `mx-red` | Cherry MX Red (ABS) | `cherrymx-red-abs` |
| `mx-brown` | Cherry MX Brown (ABS) | `cherrymx-brown-abs` |
| `topre` | Topre | `topre-purple-hybrid-pbt` |

### Optional

Order in this table is the order shown in the profile list.

| id | Display name | MechSim source dir | Shape |
|---|---|---|---|
| `mx-black` | Cherry MX Black (PBT) | `cherrymx-black-pbt` | 51 keys, 5 defaults |
| `mx-black-abs` | Cherry MX Black (ABS) | `cherrymx-black-abs` | 51 keys, 5 defaults |
| `mx-blue-pbt` | Cherry MX Blue (PBT) | `cherrymx-blue-pbt` | 51 keys, 5 defaults |
| `mx-red-pbt` | Cherry MX Red (PBT) | `cherrymx-red-pbt` | 51 keys, 5 defaults |
| `mx-brown-pbt` | Cherry MX Brown (PBT) | `cherrymx-brown-pbt` | 51 keys, 5 defaults |
| `holy-panda` | Holy Panda | `holy-pandas` | 3 keys, 5 defaults |
| `nk-cream` | NK Cream | `nk-cream` | 51 keys, 5 defaults |
| `eg-oreo` | Everglide Oreo | `eg-oreo` | 51 keys, 5 defaults |
| `eg-crystal-purple` | Everglide Crystal Purple | `eg-crystal-purple` | 51 keys, 5 defaults |
| `turquoise` | Turquoise Tealios | `turquoise` | 3 keys, 5 defaults |
| `cream-travel` | Cream (Full Travel) | `cream-travel` | 3 keys, 5 defaults |
| `mx-black-travel` | Cherry MX Black (Full Travel) | `mxblack-travel` | 3 keys, 5 defaults |
| `mx-blue-travel` | Cherry MX Blue (Full Travel) | `mxblue-travel` | 3 keys, 5 defaults |
| `mx-brown-travel` | Cherry MX Brown (Full Travel) | `mxbrown-travel` | 3 keys, 5 defaults |

All 18 packs come from `cjlangan/MechSim` (MIT). `mx-black` is the PBT
recording; `mx-black-abs` is the ABS recording. The four "Full Travel" packs
only have per-key recordings for Backspace, Enter, and Space; other keys
rotate through five generic takes.

## Asset layout

```
assets/
  CREDITS.md
  sounds/<id>/{config.json,sound.flac}             # 4 embedded
  sounds-optional/<id>/{config.json,sound.flac}    # 14 optional
```

The manifest format is unchanged (`version: 1`, mono, 44.1 kHz, 16-bit FLAC,
sample ranges). Optional packs are exactly the same shape as embedded packs, so
one decoder and one manifest parser serve both.

`scripts/build_sound_packs.py` writes the two roots. Existing directories are
moved with `git mv`; existing assets are not regenerated. The seven new packs
are generated with the script.

## Runtime cache

```
<config dir>/brokey/
  config.toml
  sounds/<name>/press.*            # user packs, unchanged
  packs/<id>/{config.json,sound.flac}  # downloaded optional packs
```

`packs/` is separate from `sounds/` so user-pack discovery and optional-pack
cache never collide. `config::dir()` already provides the base directory; a new
helper returns `dir()/packs`.

A cached pack is considered installed when both `config.json` and `sound.flac`
exist. Full validation (manifest parse plus FLAC decode) happens when the pack
is loaded. If a cache exists but fails to load, that same selection removes the
corrupt directory, shows a notice, and immediately starts a fresh download. If
the fresh download also fails, the previous pack stays active and the user can
retry by selecting the profile again. No version or checksum file is stored;
the manifest `version` field is the format guard.

## Download module (`src/download.rs`)

New module, no UI or audio dependencies beyond `pack`.

```rust
pub const BASE: &str =
    "https://raw.githubusercontent.com/ntk148v/brokey/master/assets/sounds-optional";

pub fn is_installed(packs: &Path, id: &str) -> bool;
pub fn load_cached(packs: &Path, id: &str) -> Result<LoadedPack, String>;
pub fn fetch_and_install(packs: &Path, id: &str) -> Result<(), String>;

// Exposed for tests; validates and installs bytes already in memory.
pub fn install_from_bytes(
    packs: &Path,
    id: &str,
    manifest: &[u8],
    sprite: &[u8],
) -> Result<(), String>;
```

- `ureq = "2"` is added to `[dependencies]`. It is blocking, uses rustls with
  bundled webpki roots, and does not need OpenSSL or system certificate setup.
- Profile ids are validated against `^[a-z0-9][a-z0-9-]{0,63}$` before being
  used in a URL or path. This rejects `.`, `/`, uppercase, and traversal.
- Each file is fetched with a 16 MiB cap and a 30 s request timeout. HTTP
  failures, non-200 responses, timeouts, and oversize bodies become readable
  error strings.
- `fetch_and_install` downloads `config.json` then `sound.flac`, calls
  `install_from_bytes`, and returns.
- `install_from_bytes` parses the manifest and decodes the FLAC **in memory**
  before touching the cache. On success it writes both files into a uniquely
  named `packs/.tmp-<id>-<unique>` directory. If `packs/<id>` already exists it
  is first renamed to `packs/.old-<id>-<unique>`; the temp directory is then
  renamed into place and the backup is removed. If the second rename fails, the
  backup is restored. On any failure the temp directory is removed and the
  previous cache is left untouched.
- `load_cached` reads both files, parses the manifest, and decodes with
  `LoadedPack::from_builtin`; errors include the pack id.

The base URL is a constant, not user input. Only ids from the hardcoded catalog
can be requested.

The URL points at `master`. A future release can pin the ref without changing
the API; the manifest version already protects against format drift.

## Runtime design

### Profiles (`src/profiles.rs`)

```rust
pub enum ProfileSource {
    Builtin { sprite: &'static [u8], manifest: &'static [u8] },
    Optional,
    Custom { press: Vec<u8>, space: Option<Vec<u8>> },
}
```

- `builtins()` keeps only the four embedded specs and still returns manifest
  warnings.
- `optional()` returns the 14 metadata-only profiles in table order. It cannot
  fail, so it returns `Vec<Profile>` rather than a tuple.
- `all(user_dir)` returns defaults, then optional, then custom packs. A custom
  pack whose id collides with any default or optional id is skipped with a
  warning ("conflicts with a profile").
- `Profile::is_optional()` and `Profile::is_available(packs_dir)` are small
  helpers used by startup and the UI. `is_available` is true for embedded and
  custom profiles and checks only file existence for optional ones.
- If `config::dir()` is unavailable, `packs_dir` is `None`; optional profiles
  are still listed but cannot be downloaded and report an error when selected.
  Embedded and custom packs keep working.

### Audio thread (`src/audio.rs`)

`spawn` gains a `packs_dir: Option<PathBuf>` argument.

`AudioCmd` stays `Copy + PartialEq`; only indices and success flags cross the
command channel:

```rust
pub enum AudioCmd {
    Sound(u16),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
    PackInstalled { index: usize, ok: bool },
}
```

Audio-thread state:

```rust
selected: usize,
pack: Option<LoadedPack>,
downloading: Option<String>,   // id of the active download
pending: Option<usize>,        // latest selection made while downloading
```

Selection behavior:

1. `SetProfile(i)` records `selected = i` and tries to load.
2. Embedded and custom profiles load immediately. On success the pack is
   replaced and one default clip is played as the preview, even while paused.
   The UI no longer sends a second `Preview` command.
3. Optional profiles load from cache when available. If the manifest or FLAC is
   corrupt, the cache directory is removed, a warning is shown, and the
   download path below runs immediately.
4. If an optional pack is not cached and no download is active, the audio thread
   sends `Notice::Downloading(id)` and spawns one worker thread.
5. If a download is already active and the new selection is another uncached
   optional pack, it is remembered in `pending`. A second download is not
   started. Selecting an embedded, custom, or cached profile clears `pending`.
   This prevents scrolling through the list from spawning many downloads or
   retrying a dead network repeatedly.
6. The worker calls `download::fetch_and_install`. On failure it sends
   `Notice::DownloadFailed { id, error }` and then
   `AudioCmd::PackInstalled { index, ok: false }`. On success it sends
   `PackInstalled { index, ok: true }`; the audio thread sends
   `Notice::Downloaded(id)` and reloads the pack from cache only if that index
   is still selected.
7. `PackInstalled` clears `downloading` using `profiles[index].id`. Then
   `pending` is consulted once: if the current selection is still an uncached
   optional pack, a download starts for it. A failed download clears `pending`
   instead of cascading through the list, and does not auto-retry.
8. At startup the saved profile is loaded the same way but without a preview.
   If it is an optional pack that is not cached, it downloads immediately.

Failure keeps the previous pack active.

### Notices (`src/main.rs`)

```rust
pub enum Notice {
    ListenerFailed(String),
    Warn(String),
    Downloading(String),                 // profile id
    Downloaded(String),                  // profile id
    DownloadFailed { id: String, error: String },
}
```

The UI resolves ids to display names.

### UI (`src/ui.rs`)

`App.profiles` becomes a list of view entries:

```rust
pub struct ProfileEntry {
    pub id: String,
    pub name: String,
    pub optional: bool,
    pub available: bool,
    pub downloading: bool,
}
```

- `ui::new(entries, cfg)` still selects the saved id and falls back to the
  first entry with a notice.
- `on_notice` handles the new variants: `Downloading` sets `downloading` and
  shows "Downloading <name>…"; `Downloaded` clears the flag, sets
  `available = true`, and shows "Downloaded <name>"; `DownloadFailed` clears
  the flag and shows "Could not download <name>: <error>".
- The list keeps the existing layout and `ListState` scrolling. Entries that
  are optional and unavailable carry a download marker; the exact glyph and
  styling are a UI task, with the intent that available packs look normal and
  the selected item remains visible when 18 entries overflow the list area.
- No new keybindings. Selecting an optional pack starts its download; pressing
  it again after a failure retries.
- `run()` stops sending `Preview` after `SetProfile`; the audio thread previews
  on successful load. `p` still sends `Preview` explicitly.

### Config

The schema is unchanged. The UI saves the selected profile id immediately, as
today. If the user selects an optional pack that then fails to download, the
saved config points at it and the next launch retries. This is intentional and
matches the current "save on change" behavior.

## Generator (`scripts/build_sound_packs.py`)

- Replace the single `PACKS` map with `DEFAULT_PACKS` (4) and
  `OPTIONAL_PACKS` (14), plus `OUT_DEFAULT = assets/sounds` and
  `OUT_OPTIONAL = assets/sounds-optional`.
- `build(profile, source_dir, out_root)` is otherwise unchanged.
- `--only ID` is repeatable and accepts any id from either map. A `--set
  {default,optional}` filter is available for bulk maintenance. Without
  filters, both sets are rebuilt.
- The four "Full Travel" packs use the existing `multi` path:
  three per-key files plus five generic takes expanded from
  `press/GENERIC_R{0-4}.mp3`. No new generator logic is required.
- Run the generator only for the seven new ids; leave existing committed assets
  untouched to avoid churn from ffmpeg version differences.

## Documentation

- `assets/CREDITS.md` moves up from `assets/sounds/` and covers both asset
  directories. The source table gains a Default/Optional column and the seven
  new rows. The license notice is unchanged (MIT, Connor Langan / MechVibes).
- README:
  - Split the profile list into "Default profiles (embedded)" and "Optional
    profiles (downloaded on first use)".
  - Document the cache path and the `↓` marker.
  - Note that switching to an optional pack previews it after the download
    completes.
  - Update the privacy note: the network is used only when an optional pack is
    selected for the first time; only pack files are requested; no telemetry.
  - Update the credits link.
- `docs/spec.md` needs no requirement changes; optional downloads still satisfy
  "lightweight" and "never record or store typed text".

## Error handling

- Download failure (offline, 404, timeout, oversize, bad manifest, bad FLAC):
  human-readable notice, previous pack stays active, cache unchanged, retry on
  the next explicit selection.
- Corrupt cached pack: load fails, the corrupt cache is removed, a notice is
  shown, and a fresh download starts on the same selection; if that fails, the
  old pack stays active.
- Missing config directory: optional packs report an error; embedded and custom
  packs work.
- Concurrent selections: one download at a time; only the latest selection can
  trigger the next download. No cascade after a failure.
- Quit during a download: the process exits; a partially written temp directory
  is ignored on the next run and never treated as installed.

## Testing

Unit tests:

1. Default catalog is exactly the four ids/names in order, and all four decode.
2. Optional catalog is exactly the 14 ids/names in table order, and every
   source is `ProfileSource::Optional`.
3. `all()` returns 4 + 14 + custom; a custom id may not shadow a default or an
   optional id.
4. Optional ids are rejected if unsafe (`""`, `"../x"`, `"MX-Blue"`, `"a/b"`,
   `".hidden"`, over 64 chars).
5. `install_from_bytes` with a real embedded pack's bytes installs under a
   different id; `is_installed` and `load_cached` then succeed.
6. Invalid manifest or invalid FLAC is rejected and no cache directory is left
   behind.
7. Reinstalling over an existing cache replaces it and leaves a loadable pack;
   a failed reinstall leaves the previous cache loadable.
8. `audio::load` succeeds for a cached optional profile and errors with a
   useful message when the cache is absent.
9. Every committed directory under `assets/sounds-optional/` decodes, and there
   are exactly 14 (read from `CARGO_MANIFEST_DIR`; not embedded).
10. UI: optional-unavailable marker renders; `Downloading`/`Downloaded`/
    `DownloadFailed` notices update the entry and notice line; a list with 18
    entries scrolls so the selected entry is visible; the tiny-terminal test
    still does not panic.
11. Existing audio/UI/profile/config tests are updated for the new signatures.

Verification:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --release`, compare binary size and `du -sh assets/sounds
  assets/sounds-optional`
- Manual: run offline, confirm the four defaults work and selecting an optional
  pack shows a failure without losing sound; with network, select an optional
  pack, confirm it downloads, previews, and plays keys; restart and confirm it
  loads from cache; delete its cache directory and confirm it re-downloads;
  confirm custom packs still load.
- The raw GitHub URLs only work after the assets are pushed to `origin/master`.
  Verify with `curl -I` on one `config.json` and one `sound.flac` after pushing.

## Acceptance criteria

- Four defaults are embedded and work with no network.
- Fourteen optional packs are listed; selecting an uncached one downloads it,
  caches it, previews it, and plays it.
- Cached optional packs load without network and without re-downloading.
- A failed download leaves the previous pack playing and is visible in the UI.
- No more than one download runs at a time; scrolling does not trigger a burst
  of downloads.
- Existing `config.toml` files and custom packs keep working.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo fmt --check` pass.
- All 18 committed packs decode; credits list every source and license.

## Risks

- **GitHub raw availability**: optional packs depend on
  `raw.githubusercontent.com` and the `master` branch. Failures are graceful,
  but a renamed branch or GitHub outage breaks first-time downloads. Pinning a
  release tag later is a one-line change.
- **Repository size**: the repo grows by roughly 1.5-2.5 MB from the seven new
  packs, while the binary shrinks by about the seven moved packs. Net disk use
  goes up for the repo, down for installed binaries.
- **TLS dependency**: `ureq`/rustls adds build time and some binary size, partly
  offsetting the smaller embedded assets. It avoids OpenSSL setup on all three
  platforms.
- **Cache corruption**: handled by validation on load and re-download on
  selection; there is no background repair.
- **Stale cache**: downloaded packs are not updated automatically. A future
  manifest version bump can invalidate them.
- **Generator reproducibility**: upstream MechSim files can change. Committed
  assets are the source of truth; only the seven new packs are generated now.
