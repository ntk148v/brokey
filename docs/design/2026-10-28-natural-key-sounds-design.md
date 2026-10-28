# Natural per-key typing sounds

Date: 2026-10-28
Status: Draft for review

## Problem

Every keypress currently plays the same single `press` clip per profile. Real
keyboards sound different depending on which key is struck, so brokey's output
sounds like a sound effect rather than typing.

MechVibes/MechSim publish real keyboard recordings as a sprite sheet
(`sound.ogg` + `config.json` mapping scan codes to sample offsets) or as
per-key files. Those packs reproduce natural typing but are large: a single
Cherry MX Black ABS pack is a 2.6 MB OGG sprite plus a 7 KB config.

## Goals

- Play a distinct, real recording per typing key for every built-in profile.
- Add the Cherry MX Black ABS pack the user requested.
- Keep the repository and binary small: roughly 3 MB of audio for 11 profiles
  instead of ~30 MB of source sprites.
- Preserve the current custom-pack format (`press.*` + optional `space.*`).
- Preserve privacy: only a keycode is used to choose a sound; nothing is
  recorded or stored.
- Keep startup fast and memory bounded by loading only the active pack.

## Non-goals

- Per-key sounds for custom packs (they keep one press clip plus optional space).
- Recording or synthesizing new audio.
- Changing the TUI layout, configuration format, or volume/pause semantics
  beyond the pause detail noted below.
- Shipping the generator's source sprites or requiring network access at
  build/runtime.

## Asset format

Each built-in profile gets two committed files under
`assets/sounds/<id>/`:

- `sound.flac` — mono, 44.1 kHz, 16-bit FLAC containing only the selected
  typing-key clips concatenated in ascending scan-code order. About 10 s and
  ~290 KB per pack.
- `config.json` — manifest describing where each clip lives inside
  `sound.flac`.

Manifest schema:

```json
{
  "version": 1,
  "profile": "mx-black-abs",
  "source": {
    "repo": "https://github.com/cjlangan/MechSim",
    "dir": "audio/cherrymx-black-abs",
    "license": "MIT"
  },
  "sample_rate": 44100,
  "channels": 1,
  "total_samples": 427123,
  "audio": "sound.flac",
  "keys": {
    "16": [0, 8021],
    "17": [8021, 7450]
  },
  "defaults": [[0, 8021], [8021, 7450]]
}
```

- `keys` maps a MechVibes scan code (decimal string) to
  `[start_sample, length_samples]`.
- `defaults` lists ranges used when a pressed key has no specific clip and for
  preview. Ranges are chosen by the generator (generic takes when the source
  has them, otherwise a few common letter clips).
- All offsets are sample indices into the decoded mono stream.
- Manifests contain no timestamps so regeneration is deterministic.

Decoding uses rodio's FLAC support (already enabled in `Cargo.toml`). FLAC was
chosen over MP3/AAC to avoid codec delay and over OGG because the available
ffmpeg has no Vorbis encoder. MP3 at 96 kbps would be ~78 KB per pack but can
shift clip starts; lossless offsets are worth the extra ~200 KB.

### Source packs

All 11 built-ins come from `cjlangan/MechSim` (MIT), which hosts the same
recordings as MechVibes. `cherrymx-black-abs/sound.ogg` in MechSim is
byte-identical to the MechVibes download for the requested Cherry MX Black ABS
pack, so the requested sound can be bundled under MechSim's MIT license.

| Profile id | Display name | MechSim dir |
|---|---|---|
| `mx-blue` | Cherry MX Blue | `cherrymx-blue-abs` |
| `mx-red` | Cherry MX Red | `cherrymx-red-abs` |
| `mx-brown` | Cherry MX Brown | `cherrymx-brown-abs` |
| `topre` | Topre | `topre-purple-hybrid-pbt` |
| `mx-black` | Cherry MX Black | `cherrymx-black-pbt` |
| `mx-black-abs` | Cherry MX Black (ABS) | `cherrymx-black-abs` (new) |
| `holy-panda` | Holy Panda | `holy-pandas` |
| `nk-cream` | NK Cream | `nk-cream` |
| `eg-oreo` | Everglide Oreo | `eg-oreo` |
| `eg-crystal-purple` | Everglide Crystal Purple | `eg-crystal-purple` |
| `turquoise` | Turquoise Tealios | `turquoise` |

Existing ids and display names stay stable so saved configs keep working.
`mx-black` moves to the PBT recording so it remains distinct from the new ABS
profile.

### Selected keys

The generator extracts these 51 scan codes (letters, digits, common
punctuation, Space, Enter, Backspace, Tab):

- letters: 16–25, 30–38, 44–50
- digits: 2–11
- punctuation: 12, 13, 26, 27, 39, 40, 41, 43, 51, 52, 53
- specials: 14 (Backspace), 15 (Tab), 28 (Enter), 57 (Space)

MechVibes configs also contain release entries (keys suffixed `-up`); those are
ignored. Other keys (arrows, modifiers, function keys) are still classified at
runtime and fall back to `defaults`.

## Generator

`scripts/build_sound_packs.py` (Python 3 standard library + `ffmpeg`) is the
only new build-time tool. It is not used by `cargo build` or at runtime.

For each profile:

1. Download `config.json` and either `sound.ogg` or per-key files from
   `raw.githubusercontent.com/cjlangan/MechSim/main/audio/<dir>/`.
2. Handle both config shapes:
   - `key_define_type: "single"`: slice `sound.ogg` by the `[start_ms,
     duration_ms]` ranges in `defines`.
   - `key_define_type: "multi"`: load the file named by each `defines` entry;
     `GENERIC_R*` files become `defaults`.
3. Decode each clip to mono 44.1 kHz f32 with ffmpeg, apply a 2 ms fade-in and
   6 ms fade-out, and append it to the pack buffer in ascending scan-code
   order.
4. Apply one gain across the whole pack so the peak is −2 dBFS (preserves
   relative loudness between keys), then encode `sound.flac` with
   `-c:a flac -compression_level 8 -sample_fmt s16`.
5. Write `config.json` with offsets computed from the concatenated buffer and
   print per-pack and total sizes.

Defaults selection: use the source's generic takes when present; otherwise use
the clips for A, S, D, F, G that exist in the pack (or the first five clips).

Usage: `python3 scripts/build_sound_packs.py [--only <id>]`. Output is written
directly to `assets/sounds/<id>/`; generated files are committed. The script
must be re-runnable and produce identical output for identical inputs.

## Runtime design

### Profiles (`src/profiles.rs`)

```rust
pub enum ProfileSource {
    Builtin {
        sprite: &'static [u8],
        manifest: &'static [u8],
    },
    Custom {
        press: Vec<u8>,
        space: Option<Vec<u8>>,
    },
}

pub struct Profile {
    pub id: String,
    pub name: String,
    pub source: ProfileSource,
}
```

- `builtins()` uses `include_bytes!` for each `sound.flac` and `config.json`.
- `load_user(dir)` keeps the current discovery rules: required `press.*`,
  optional `space.*`, supported extensions unchanged, invalid packs warn and
  are skipped, and custom ids cannot shadow built-ins.
- `all(user_dir)` still returns built-ins followed by user packs.
- A new module `src/pack.rs` owns manifest parsing and validation so
  `profiles.rs` stays about discovery.

### Manifest and pack decoding (`src/pack.rs`)

```rust
pub struct Manifest {
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: usize,
    pub keys: HashMap<u16, (usize, usize)>,
    pub defaults: Vec<(usize, usize)>,
}

pub struct PlaybackClip {
    pub samples: Arc<Vec<f32>>,
    pub channels: u16,
    pub rate: u32,
    pub start: usize,
    pub len: usize,
}

pub struct LoadedPack {
    pub keys: HashMap<u16, PlaybackClip>,
    pub defaults: Vec<PlaybackClip>,
    next_default: usize,
}
```

- `Manifest::parse(bytes)` deserializes with serde and rejects
  `version != 1`, zero sample rate, non-mono built-ins, empty defaults, and any
  range whose end exceeds `total_samples` or whose length is zero.
- `LoadedPack::from_builtin(sprite, manifest)` decodes the FLAC once into a
  shared `Arc<Vec<f32>>` and builds clips that borrow ranges of it.
- `LoadedPack::from_custom(press, space)` decodes each file separately (its own
  rate/channels are preserved), maps scan code 57 to the space clip when
  present, and sets `defaults` to the press clip. Custom packs therefore get
  the same per-key machinery without a manifest.
- `LoadedPack::clip_for(scan_code)` returns the specific clip or
  `next_default()`; `next_default()` round-robins `defaults` with a wrapping
  counter (no RNG dependency).
- Decoding failures produce an error string; the caller warns and skips the
  profile instead of panicking.

### Audio thread (`src/audio.rs`)

- `AudioCmd::Sound` now carries a `u16` scan code; `SoundKind` is removed.
- The thread keeps the current `LoadedPack` and reloads it on `SetProfile`.
  Only the active pack is decoded, keeping memory at ~1.7 MB plus the encoded
  bytes.
- Playback uses `OutputStreamHandle::play_raw` with
  `SamplesBuffer::new(channels, rate, slice).amplify(volume)` so overlapping
  keypresses mix instead of queueing behind each other. Volume changes apply
  to subsequent clips.
- Pause is handled by ignoring `Sound` and `Preview` commands while paused.
  Profile switches still load; resume simply starts accepting sounds again.
  This replaces the sink-level pause so `play_raw` can be used.
- `Preview` plays `next_default()` once.
- The audio stream itself is still created once; stream errors keep the
  current "audio unavailable" behavior.

### Listener (`src/listener.rs`)

- `classify(&mut Vec<Key>, &EventType) -> Option<u16>` keeps the existing
  held-key dedupe for auto-repeat and ignores non-keyboard events.
- A `key_to_scan_code(rdev::Key) -> Option<u16>` table covers every key that
  MechVibes defines: letters, digits, punctuation, Space/Enter/Backspace/Tab,
  Escape, arrows, modifiers, function keys, navigation keys, and keypad.
  `Unknown`/`Function` return `None`.
- Representative mappings (full table follows MechVibes `keycodes.js`):
  `KeyA`→30, `KeyZ`→44, `Num1`→2, `Num0`→11, `Minus`→12, `Equal`→13,
  `LeftBracket`→26, `SemiColon`→39, `Quote`→40, `BackQuote`→41,
  `BackSlash`→43, `Comma`→51, `Dot`→52, `Slash`→53, `Space`→57,
  `Return`→28, `Backspace`→14, `Tab`→15, `Escape`→1, `UpArrow`→57416,
  `LeftArrow`→57419, `RightArrow`→57421, `DownArrow`→57424.

### UI and config

- `App` receives the 11 profiles and needs no structural change; the list
  already scrolls with `ListState`.
- Selecting a profile still sends `SetProfile` then `Preview`, so the user
  hears the new pack immediately.
- `config.rs` is unchanged. If the saved profile id is missing (for example a
  removed user pack), startup falls back to the first built-in.

## Error handling

- Missing/corrupt built-in asset: warn to stderr, skip that profile, keep the
  rest usable. Built-ins are compile-time embedded, so this indicates a build
  defect.
- Invalid manifest or FLAC decode error: warn, skip the profile.
- Custom pack failure: unchanged behavior (warn and skip).
- Audio output unavailable: unchanged; UI remains usable.
- No network at generation time: script fails with the failing URL and leaves
  existing committed assets untouched.

## Privacy

The listener uses the key identity only to select a sample. It does not build,
log, or persist text, and no audio is recorded. README wording changes from
"only knows that a key was pressed, never which character" to "uses the
keycode only to pick a sound; nothing is recorded or stored" because the
keycode is now observed.

## Documentation

- README: add `mx-black-abs` to the built-in list, document that built-ins have
  per-key sounds and fall back for unrecorded keys, and update the privacy
  sentence. Custom pack instructions stay the same.
- `assets/sounds/CREDITS.md`: list all 11 MechSim source dirs, the MIT license,
  the FLAC conversion pipeline, mono/44.1 kHz/16-bit format, and the
  fade/normalization settings. Note that the Cherry MX Black ABS recording is
  the same one distributed by MechVibes.
- `docs/spec.md`: no requirement changes; confirm the privacy and lightweight
  requirements remain satisfied.

## Testing

Unit/integration tests (`cargo test`):

1. `key_to_scan_code` maps representative keys from each group and returns
   `None` for `Unknown`.
2. Auto-repeat dedupe still emits one code per physical press.
3. Manifest parsing accepts a valid manifest and rejects out-of-range,
   zero-length, wrong-version, and empty-default cases.
4. `LoadedPack` returns the specific clip for a known key, falls back for an
   unknown key, and rotates through defaults.
5. Every built-in decodes: parse its manifest, decode its FLAC, and assert all
   ranges are in bounds and `total_samples` matches the decoded length.
6. A temporary custom pack (`press.wav` + `space.wav`) loads, maps scan code
   57 to space, and falls back to press for letters.
7. UI smoke test with `TestBackend` renders all 11 names including
   "Cherry MX Black (ABS)".

Verification commands:

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release` and a manual run: switch profiles, type, verify
  different letters sound different, Space uses the space clip, and pause,
  volume, preview, and quit still work.

## Acceptance criteria

- 11 built-in profiles are listed; `mx-black-abs` plays the requested Cherry
  MX Black ABS recording.
- Typing different letters in any built-in profile produces different samples;
  Space uses the space slice when the pack has one.
- Keys without a specific slice still produce a click via defaults.
- Total committed audio is under 4 MB.
- Existing custom packs load and play unchanged.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- No typed characters or text are logged or stored.

## Risks

- **rodio API drift**: `Decoder` item type and `play_raw`/`amplify` signatures
  differ across 0.21 patch releases. The implementation plan verifies the
  exact signatures before coding.
- **Licensing**: recordings originate from MechVibes/MechSim. Both are MIT;
  CREDITS must name both projects and the source directories.
- **Generator reproducibility**: upstream MechSim files can change. Committed
  assets are the source of truth; regeneration is only for maintenance.
- **Keycap variants**: `mx-black` switches from its old asset to the PBT
  recording. This is intentional to keep it distinct from the new ABS profile
  and is documented in CREDITS.
- **Lazy decode latency**: decoding ~290 KB FLAC on profile switch takes a few
  milliseconds and happens while no typing sound is playing; acceptable.
