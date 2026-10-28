# Optional On-Demand Sound Packs Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed only four sound packs and download the other fourteen from GitHub on first selection, caching them under the user config directory.

**Architecture:** A new `download` module fetches and validates prebuilt packs into `packs/<id>/` with atomic directory replacement. `ProfileSource::Optional` makes the catalog metadata-only. The audio thread owns a single-download state machine and sends UI notices; the UI shows availability markers and keeps the old pack playing on failure.

**Tech Stack:** Rust 2021, ratatui 0.29, rodio 0.21, ureq 2 (rustls), serde/serde_json, Python 3 + ffmpeg for asset generation.

**Spec:** `docs/design/2026-10-28-optional-sound-packs-design.md`

**Repo:** `/Users/kiennt/Workspace/github.com/ntk148v/brokey`, branch `master`. Baseline: `cargo test` = 40 passed.

---

## File map

| File | Change |
|---|---|
| `Cargo.toml` | Add `ureq = "2"` |
| `src/download.rs` | New: fetch, validate, cache, atomic install |
| `src/profiles.rs` | 4 embedded + 14 optional catalog, availability helpers |
| `src/audio.rs` | Optional loading, single-download state machine, preview on select |
| `src/main.rs` | New `Notice` variants, `packs_dir`, `ProfileEntry` wiring |
| `src/ui.rs` | `ProfileEntry`, download markers, new notices |
| `scripts/build_sound_packs.py` | Split default/optional outputs, add 7 sources |
| `assets/sounds/` | Keep only 4 packs; credits move out |
| `assets/sounds-optional/` | 14 packs (7 moved, 7 new) |
| `assets/CREDITS.md` | Moved and expanded |
| `README.md` | Default vs optional profiles, cache path, privacy |

Execution lanes (non-overlapping writes):
- **Lane A (fixer):** Tasks 1-2 (`src/download.rs`, `Cargo.toml`, `src/profiles.rs`).
- **Lane B (fixer):** Tasks 6-7 (`scripts/build_sound_packs.py`, assets, credits).
- **Lane C (designer):** Task 5 (`src/ui.rs`) after the types in Tasks 2 and 4 are fixed; may start from this plan.
- **Lane D (fixer):** Tasks 3-4 (`src/audio.rs`, `src/main.rs`) after Lane A.
- **Lane E (fixer/docs):** Task 8 after assets and names land.
- **Orchestrator:** reconcile, Task 9, commits.

---

## Chunk 1: Core runtime

### Task 1: Download module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/download.rs`
- Test: `src/download.rs` (`#[cfg(test)]` inline)

- [ ] **Step 1: Add the dependency**

Add under `[dependencies]` in `Cargo.toml`:

```toml
ureq = "2"
```

Run `cargo fetch` and expect success.

- [ ] **Step 2: Declare the module**

In `src/main.rs`, add `pub mod download;` next to the other `pub mod` lines:

```rust
pub mod audio;
pub mod config;
pub mod download;
pub mod listener;
pub mod pack;
pub mod profiles;
pub mod ui;
```

- [ ] **Step 3: Write the failing tests**

Create `src/download.rs` with the implementation from Step 5 **minus the function bodies** (use `todo!()`), then append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::{builtins, ProfileSource};
    use tempfile::tempdir;

    fn embedded() -> (&'static [u8], &'static [u8]) {
        let (profiles, warnings) = builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        match &profiles[0].source {
            ProfileSource::Builtin { sprite, manifest } => (*sprite, *manifest),
            _ => panic!("first builtin is not embedded"),
        }
    }

    #[test]
    fn accepts_catalog_ids_and_rejects_paths() {
        for id in ["mx-blue-pbt", "a", "0", "a-b9", "mx-black-travel"] {
            assert!(valid_id(id), "{id} should be valid");
        }
        for id in [
            "",
            "../x",
            "MX-Blue",
            "a/b",
            ".hidden",
            "a b",
            "-leading",
            "trailing-",
        ] {
            assert!(!valid_id(id), "{id} should be invalid");
        }
        assert!(!valid_id(&"a".repeat(65)));
    }

    #[test]
    fn is_installed_rejects_invalid_ids() {
        let td = tempdir().unwrap();
        assert!(!is_installed(td.path(), "../x"));
        assert!(!is_installed(td.path(), "mx-blue-pbt"));
    }

    #[test]
    fn install_from_bytes_roundtrips() {
        let td = tempdir().unwrap();
        let (sprite, manifest) = embedded();
        install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();

        assert!(is_installed(td.path(), "mx-blue-pbt"));
        let pack = load_cached(td.path(), "mx-blue-pbt").unwrap();
        assert!(!pack.keys.is_empty());
        assert!(!pack.defaults.is_empty());
    }

    #[test]
    fn invalid_manifest_leaves_no_cache() {
        let td = tempdir().unwrap();
        let (sprite, _) = embedded();
        let err = install_from_bytes(td.path(), "bad", b"{}", sprite).unwrap_err();
        assert!(err.contains("manifest"), "{err}");
        assert!(!td.path().join("bad").exists());
    }

    #[test]
    fn invalid_audio_leaves_no_cache() {
        let td = tempdir().unwrap();
        let (_, manifest) = embedded();
        let err = install_from_bytes(td.path(), "bad", manifest, b"not flac").unwrap_err();
        assert!(err.contains("audio"), "{err}");
        assert!(!td.path().join("bad").exists());
    }

    #[test]
    fn failed_reinstall_keeps_previous_cache() {
        let td = tempdir().unwrap();
        let (sprite, manifest) = embedded();
        install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();
        assert!(load_cached(td.path(), "mx-blue-pbt").is_ok());

        let err = install_from_bytes(td.path(), "mx-blue-pbt", b"{}", b"nope").unwrap_err();
        assert!(err.contains("manifest"), "{err}");
        assert!(load_cached(td.path(), "mx-blue-pbt").is_ok());
    }

    #[test]
    fn reinstall_replaces_cache() {
        let td = tempdir().unwrap();
        let (sprite, manifest) = embedded();
        install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();
        install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();
        assert!(load_cached(td.path(), "mx-blue-pbt").is_ok());
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test download::tests 2>&1 | tail -30`
Expected: compile error because `valid_id`, `install_from_bytes`, etc. are `todo!()` or missing.

- [ ] **Step 5: Implement `src/download.rs`**

```rust
use crate::pack::{LoadedPack, Manifest};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BASE: &str =
    "https://raw.githubusercontent.com/ntk148v/brokey/master/assets/sounds-optional";

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

pub fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_lowercase() || b.is_ascii_digit() => {}
        _ => return false,
    }
    id.len() <= 64 && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn cache_dir(packs: &Path, id: &str) -> PathBuf {
    packs.join(id)
}

pub fn is_installed(packs: &Path, id: &str) -> bool {
    if !valid_id(id) {
        return false;
    }
    let dir = cache_dir(packs, id);
    dir.join("config.json").is_file() && dir.join("sound.flac").is_file()
}

pub fn load_cached(packs: &Path, id: &str) -> Result<LoadedPack, String> {
    if !valid_id(id) {
        return Err(format!("invalid pack id '{id}'"));
    }
    let dir = cache_dir(packs, id);
    let manifest = fs::read(dir.join("config.json"))
        .map_err(|e| format!("could not read cached manifest for '{id}': {e}"))?;
    let sprite = fs::read(dir.join("sound.flac"))
        .map_err(|e| format!("could not read cached audio for '{id}': {e}"))?;
    LoadedPack::from_builtin(&sprite, &manifest)
        .map_err(|e| format!("could not decode cached pack '{id}': {e}"))
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .timeout(TIMEOUT)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    let mut out = Vec::new();
    response
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("GET {url}: {e}"))?;
    if out.len() as u64 > MAX_BYTES {
        return Err(format!("GET {url}: response is larger than {MAX_BYTES} bytes"));
    }
    Ok(out)
}

pub fn fetch_and_install(packs: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("invalid pack id '{id}'"));
    }
    let manifest = fetch(&format!("{BASE}/{id}/config.json"))?;
    let sprite = fetch(&format!("{BASE}/{id}/sound.flac"))?;
    install_from_bytes(packs, id, &manifest, &sprite)
}

pub fn install_from_bytes(
    packs: &Path,
    id: &str,
    manifest: &[u8],
    sprite: &[u8],
) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("invalid pack id '{id}'"));
    }
    Manifest::parse(manifest)
        .map_err(|e| format!("pack '{id}' has an invalid manifest: {e}"))?;
    LoadedPack::from_builtin(sprite, manifest)
        .map_err(|e| format!("pack '{id}' has invalid audio: {e}"))?;

    fs::create_dir_all(packs)
        .map_err(|e| format!("could not create {}: {e}", packs.display()))?;

    let unique = unique_suffix();
    let tmp = packs.join(format!(".tmp-{id}-{unique}"));
    let backup = packs.join(format!(".old-{id}-{unique}"));
    let target = cache_dir(packs, id);

    let installed = (|| -> Result<(), String> {
        fs::create_dir_all(&tmp)
            .map_err(|e| format!("could not create {}: {e}", tmp.display()))?;
        fs::write(tmp.join("config.json"), manifest)
            .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        fs::write(tmp.join("sound.flac"), sprite)
            .map_err(|e| format!("could not write {}: {e}", tmp.display()))?;

        let had_previous = target.exists();
        if had_previous {
            fs::rename(&target, &backup)
                .map_err(|e| format!("could not replace {}: {e}", target.display()))?;
        }
        match fs::rename(&tmp, &target) {
            Ok(()) => {
                if had_previous {
                    let _ = fs::remove_dir_all(&backup);
                }
                Ok(())
            }
            Err(e) => {
                if had_previous {
                    let _ = fs::rename(&backup, &target);
                }
                Err(format!("could not install {}: {e}", target.display()))
            }
        }
    })();

    if installed.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    installed
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test download::tests`
Expected: 7 tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/download.rs
git commit -m "feat: add validated on-demand pack downloader"
```

---

### Task 2: Catalog and availability

**Files:**
- Modify: `src/profiles.rs`
- Test: `src/profiles.rs` (`#[cfg(test)]` inline)

- [ ] **Step 1: Write the failing catalog tests**

Replace the existing `builtins_are_the_expected_switches` and `all_builtins_decode` tests, and add:

```rust
#[test]
fn builtins_are_the_expected_defaults() {
    let (b, warnings) = builtins();
    assert!(warnings.is_empty(), "{warnings:?}");
    let ids: Vec<&str> = b.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["mx-blue", "mx-red", "mx-brown", "topre"]);
    let names: Vec<&str> = b.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Cherry MX Blue (ABS)",
            "Cherry MX Red (ABS)",
            "Cherry MX Brown (ABS)",
            "Topre",
        ]
    );
}

#[test]
fn optional_catalog_is_the_expected_order() {
    let o = optional();
    let ids: Vec<&str> = o.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "mx-black",
            "mx-black-abs",
            "mx-blue-pbt",
            "mx-red-pbt",
            "mx-brown-pbt",
            "holy-panda",
            "nk-cream",
            "eg-oreo",
            "eg-crystal-purple",
            "turquoise",
            "cream-travel",
            "mx-black-travel",
            "mx-blue-travel",
            "mx-brown-travel",
        ]
    );
    let names: Vec<&str> = o.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Cherry MX Black (PBT)",
            "Cherry MX Black (ABS)",
            "Cherry MX Blue (PBT)",
            "Cherry MX Red (PBT)",
            "Cherry MX Brown (PBT)",
            "Holy Panda",
            "NK Cream",
            "Everglide Oreo",
            "Everglide Crystal Purple",
            "Turquoise Tealios",
            "Cream (Full Travel)",
            "Cherry MX Black (Full Travel)",
            "Cherry MX Blue (Full Travel)",
            "Cherry MX Brown (Full Travel)",
        ]
    );
    assert!(o.iter().all(|p| p.is_optional()));
}

#[test]
fn availability_tracks_the_cache() {
    let td = tempdir().unwrap();
    let (sprite, manifest) = crate::profiles::tests::embedded_bytes();
    crate::download::install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();
    let b = builtins().0;
    assert!(b[0].is_available(None));
    let o = optional();
    let p = o.iter().find(|p| p.id == "mx-blue-pbt").unwrap();
    assert!(p.is_available(Some(td.path())));
    assert!(!p.is_available(None));
    assert!(!o.iter().find(|p| p.id == "holy-panda").unwrap().is_available(Some(td.path())));
}

#[test]
fn all_lists_defaults_then_optional_then_users() {
    let td = tempdir().unwrap();
    let pack = td.path().join("zzz-mine");
    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
    let (allp, warns) = all(Some(td.path()));
    assert_eq!(allp.len(), 19, "{warns:?}");
    assert_eq!(allp[18].id, "zzz-mine");
}

#[test]
fn user_pack_cannot_shadow_optional() {
    let td = tempdir().unwrap();
    let pack = td.path().join("holy-panda");
    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
    let (allp, warns) = all(Some(td.path()));
    assert_eq!(allp.len(), 18);
    assert!(warns.iter().any(|w| w.contains("conflicts")));
}
```

Change the test module declaration in `src/profiles.rs` from `#[cfg(test)] mod tests` to `#[cfg(test)] pub(crate) mod tests`, and add this test-only helper inside it:

```rust
pub(crate) fn embedded_bytes() -> (&'static [u8], &'static [u8]) {
    let (profiles, _) = builtins();
    match &profiles[0].source {
        ProfileSource::Builtin { sprite, manifest } => (*sprite, *manifest),
        _ => panic!("first builtin is not embedded"),
    }
}
```

Also update the existing `user_pack_cannot_shadow_builtin` assertion from `builtins().0.len()` to the full catalog size:

```rust
let (allp, warns) = all(Some(td.path()));
assert_eq!(allp.len(), 18);
assert!(warns.iter().any(|w| w.contains("conflicts")));
```

Update `all_builtins_decode` to match only `Builtin` and keep the four default profiles:

```rust
#[test]
fn all_default_builtins_decode() {
    let (b, warnings) = builtins();
    assert!(warnings.is_empty(), "{warnings:?}");
    for profile in &b {
        match &profile.source {
            ProfileSource::Builtin { sprite, manifest } => {
                let pack = LoadedPack::from_builtin(sprite, manifest)
                    .unwrap_or_else(|e| panic!("{}: {e}", profile.id));
                assert!(!pack.keys.is_empty(), "{} has no keys", profile.id);
                assert!(!pack.defaults.is_empty(), "{} has no defaults", profile.id);
            }
            _ => panic!("{} is not embedded", profile.id),
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test profiles::tests 2>&1 | tail -30`
Expected: compile errors for missing `optional`, `is_optional`, `is_available`, or assertion failures for the old 11-item catalog.

- [ ] **Step 3: Implement the catalog**

In `src/profiles.rs`, change the enum:

```rust
#[derive(Clone)]
pub enum ProfileSource {
    Builtin {
        sprite: &'static [u8],
        manifest: &'static [u8],
    },
    Optional,
    Custom {
        press: Vec<u8>,
        space: Option<Vec<u8>>,
    },
}
```

Add methods after `Profile`:

```rust
impl Profile {
    pub fn is_optional(&self) -> bool {
        matches!(&self.source, ProfileSource::Optional)
    }

    pub fn is_available(&self, packs: Option<&Path>) -> bool {
        match &self.source {
            ProfileSource::Optional => packs
                .map(|dir| crate::download::is_installed(dir, &self.id))
                .unwrap_or(false),
            ProfileSource::Builtin { .. } | ProfileSource::Custom { .. } => true,
        }
    }
}
```

Replace the 11-entry `specs` array in `builtins()` with:

```rust
let specs: [(&str, &str, &'static [u8], &'static [u8]); 4] = [
    (
        "mx-blue",
        "Cherry MX Blue (ABS)",
        include_bytes!("../assets/sounds/mx-blue/sound.flac"),
        include_bytes!("../assets/sounds/mx-blue/config.json"),
    ),
    (
        "mx-red",
        "Cherry MX Red (ABS)",
        include_bytes!("../assets/sounds/mx-red/sound.flac"),
        include_bytes!("../assets/sounds/mx-red/config.json"),
    ),
    (
        "mx-brown",
        "Cherry MX Brown (ABS)",
        include_bytes!("../assets/sounds/mx-brown/sound.flac"),
        include_bytes!("../assets/sounds/mx-brown/config.json"),
    ),
    (
        "topre",
        "Topre",
        include_bytes!("../assets/sounds/topre/sound.flac"),
        include_bytes!("../assets/sounds/topre/config.json"),
    ),
];
```

Add `optional()`:

```rust
pub fn optional() -> Vec<Profile> {
    const SPECS: [(&str, &str); 14] = [
        ("mx-black", "Cherry MX Black (PBT)"),
        ("mx-black-abs", "Cherry MX Black (ABS)"),
        ("mx-blue-pbt", "Cherry MX Blue (PBT)"),
        ("mx-red-pbt", "Cherry MX Red (PBT)"),
        ("mx-brown-pbt", "Cherry MX Brown (PBT)"),
        ("holy-panda", "Holy Panda"),
        ("nk-cream", "NK Cream"),
        ("eg-oreo", "Everglide Oreo"),
        ("eg-crystal-purple", "Everglide Crystal Purple"),
        ("turquoise", "Turquoise Tealios"),
        ("cream-travel", "Cream (Full Travel)"),
        ("mx-black-travel", "Cherry MX Black (Full Travel)"),
        ("mx-blue-travel", "Cherry MX Blue (Full Travel)"),
        ("mx-brown-travel", "Cherry MX Brown (Full Travel)"),
    ];
    SPECS
        .into_iter()
        .map(|(id, name)| Profile {
            id: id.to_string(),
            name: name.to_string(),
            source: ProfileSource::Optional,
        })
        .collect()
}
```

In `all()`, append optional profiles before user packs and change the collision warning:

```rust
pub fn all(user_dir: Option<&Path>) -> (Vec<Profile>, Vec<String>) {
    let (mut result, mut warnings) = builtins();
    result.extend(optional());
    if let Some(dir) = user_dir {
        let (users, mut uw) = load_user(dir);
        for u in users {
            if result.iter().any(|p| p.id == u.id) {
                warnings.push(format!(
                    "Skipped sound pack '{}': conflicts with a profile",
                    u.id
                ));
                continue;
            }
            result.push(u);
        }
        warnings.append(&mut uw);
    }
    (result, warnings)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test profiles::tests`
Expected: 11 tests pass (the old count plus the new catalog tests, minus replaced ones).

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs
git commit -m "feat: split embedded and optional sound pack catalogs"
```

---

### Task 3: Audio download state machine

**Files:**
- Modify: `src/audio.rs`
- Test: `src/audio.rs` (`#[cfg(test)]` inline)

- [ ] **Step 1: Add the notice variants and a temporary UI arm**

In `src/main.rs`, extend `Notice`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    ListenerFailed(String),
    Warn(String),
    Downloading(String),
    Downloaded(String),
    DownloadFailed { id: String, error: String },
}
```

In `src/ui.rs`, add a temporary arm to `App::on_notice` so the new variants compile; Task 5 replaces it with real behavior:

```rust
Notice::Downloading(_) | Notice::Downloaded(_) | Notice::DownloadFailed { .. } => {}
```

- [ ] **Step 2: Write the failing state tests**

Append to the existing `mod tests` in `src/audio.rs`:

```rust
fn state_fixture() -> (tempfile::TempDir, Vec<Profile>, usize) {
    let td = tempfile::tempdir().unwrap();
    let (sprite, manifest) = crate::profiles::tests::embedded_bytes();
    crate::download::install_from_bytes(td.path(), "holy-panda", manifest, sprite).unwrap();
    let (profiles, _) = crate::profiles::all(None);
    let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
    (td, profiles, idx)
}

fn state(profiles: Vec<Profile>, packs: &std::path::Path) -> (State, std::sync::mpsc::Receiver<crate::Notice>) {
    let (ntx, nrx) = std::sync::mpsc::channel();
    let (tx, _trx) = std::sync::mpsc::channel();
    (
        State::new(profiles, Some(packs.to_path_buf()), ntx, tx, 60),
        nrx,
    )
}

#[test]
fn select_cached_optional_loads_and_previews() {
    let (td, profiles, idx) = state_fixture();
    let (mut st, _nrx) = state(profiles, td.path());
    assert!(st.select(idx, true));
    assert!(st.pack.is_some());
    assert_eq!(st.selected, idx);
}

#[test]
fn installed_success_loads_selected_and_reports() {
    let (td, profiles, idx) = state_fixture();
    let (mut st, nrx) = state(profiles, td.path());
    st.selected = idx;
    st.downloading = Some("holy-panda".to_string());
    st.preview_on_install = true;
    assert!(st.installed(idx, true));
    assert!(st.pack.is_some());
    assert!(st.downloading.is_none());
    assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloaded(id)) if id == "holy-panda"));
}

#[test]
fn installed_failure_keeps_previous_pack_and_clears_pending() {
    let (td, profiles, idx) = state_fixture();
    let (mut st, _nrx) = state(profiles, td.path());
    assert!(!st.select(0, false));
    assert!(st.pack.is_some());
    st.pending = Some(idx);
    st.downloading = Some("holy-panda".to_string());
    assert!(!st.installed(idx, false));
    assert!(st.pack.is_some());
    assert!(st.pending.is_none());
    assert!(st.downloading.is_none());
}

#[test]
fn installed_when_not_selected_does_not_replace_pack() {
    let (td, profiles, idx) = state_fixture();
    let (mut st, nrx) = state(profiles, td.path());
    st.selected = 0;
    st.downloading = Some("holy-panda".to_string());
    st.preview_on_install = true;
    assert!(!st.installed(idx, true));
    assert!(st.pack.is_none());
    assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloaded(id)) if id == "holy-panda"));
}
```

`crate::profiles::tests::embedded_bytes` must be `pub(crate)` inside `profiles::tests`; the `profiles` module must be declared `pub mod profiles;` (it is). Task 2 already changed `mod tests` to `pub(crate) mod tests` and the helper to `pub(crate) fn embedded_bytes()`. This is test-only; keep it behind `#[cfg(test)]`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test audio::tests 2>&1 | tail -30`
Expected: compile errors for missing `State`, `installed`, `preview_on_install`, etc.

- [ ] **Step 4: Implement the audio changes**

Replace the top of `src/audio.rs` through `spawn` with:

```rust
use crate::download;
use crate::pack::{LoadedPack, PlaybackClip};
use crate::profiles::{Profile, ProfileSource};
use crate::Notice;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStreamBuilder, Source};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(u16),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
    PackInstalled { index: usize, ok: bool },
}

fn load(profile: &Profile, packs: Option<&Path>) -> Result<LoadedPack, String> {
    match &profile.source {
        ProfileSource::Builtin { sprite, manifest } => LoadedPack::from_builtin(sprite, manifest),
        ProfileSource::Custom { press, space } => {
            LoadedPack::from_custom(press.clone(), space.clone())
        }
        ProfileSource::Optional => {
            let packs = packs.ok_or_else(|| "no config directory".to_string())?;
            download::load_cached(packs, &profile.id)
        }
    }
}

struct State {
    profiles: Vec<Profile>,
    packs_dir: Option<PathBuf>,
    notices: Sender<Notice>,
    tx: Sender<AudioCmd>,
    selected: usize,
    volume: u8,
    paused: bool,
    pack: Option<LoadedPack>,
    downloading: Option<String>,
    pending: Option<usize>,
    preview_on_install: bool,
}

impl State {
    fn new(
        profiles: Vec<Profile>,
        packs_dir: Option<PathBuf>,
        notices: Sender<Notice>,
        tx: Sender<AudioCmd>,
        volume: u8,
    ) -> Self {
        Self {
            profiles,
            packs_dir,
            notices,
            tx,
            selected: 0,
            volume,
            paused: false,
            pack: None,
            downloading: None,
            pending: None,
            preview_on_install: false,
        }
    }

    /// Returns true when the caller should play a preview now.
    fn select(&mut self, index: usize, preview: bool) -> bool {
        let Some(profile) = self.profiles.get(index) else {
            return false;
        };
        let name = profile.name.clone();
        let id = profile.id.clone();
        let optional = profile.is_optional();
        self.selected = index;
        self.preview_on_install = preview;

        if !optional {
            self.pending = None;
            return match load(&self.profiles[index], self.packs_dir.as_deref()) {
                Ok(pack) => {
                    self.pack = Some(pack);
                    preview
                }
                Err(e) => {
                    self.warn(format!("Could not load '{name}': {e}"));
                    false
                }
            };
        }

        let Some(packs) = self.packs_dir.clone() else {
            self.warn(format!("Could not load '{name}': no config directory"));
            return false;
        };

        match download::load_cached(&packs, &id) {
            Ok(pack) => {
                self.pending = None;
                self.pack = Some(pack);
                preview
            }
            Err(e) => {
                if download::is_installed(&packs, &id) {
                    let _ = std::fs::remove_dir_all(download::cache_dir(&packs, &id));
                    self.warn(format!("Refreshing '{name}': {e}"));
                }
                if self.downloading.is_some() {
                    self.pending = Some(index);
                } else {
                    self.start_download(index, packs);
                }
                false
            }
        }
    }

    /// Returns true when the caller should play a preview now.
    fn installed(&mut self, index: usize, ok: bool) -> bool {
        let id = self
            .profiles
            .get(index)
            .map(|p| p.id.clone())
            .unwrap_or_default();
        if self.downloading.as_deref() == Some(id.as_str()) {
            self.downloading = None;
        }
        if !ok {
            self.pending = None;
            return false;
        }

        let _ = self.notices.send(Notice::Downloaded(id.clone()));
        let mut preview = false;
        if index == self.selected {
            let name = self
                .profiles
                .get(index)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            match self
                .packs_dir
                .as_deref()
                .and_then(|p| download::load_cached(p, &id).ok())
            {
                Some(pack) => {
                    self.pack = Some(pack);
                    preview = self.preview_on_install;
                }
                None => self.warn(format!("Could not load '{name}'")),
            }
        }

        if let Some(next) = self.pending.take() {
            if next == self.selected
                && self
                    .profiles
                    .get(next)
                    .map(Profile::is_optional)
                    .unwrap_or(false)
                && !self.is_cached(next)
            {
                if let Some(packs) = self.packs_dir.clone() {
                    self.start_download(next, packs);
                }
            }
        }
        preview
    }

    fn start_download(&mut self, index: usize, packs: PathBuf) {
        let Some(id) = self.profiles.get(index).map(|p| p.id.clone()) else {
            return;
        };
        self.downloading = Some(id.clone());
        let _ = self.notices.send(Notice::Downloading(id.clone()));
        let tx = self.tx.clone();
        let notices = self.notices.clone();
        std::thread::spawn(move || match download::fetch_and_install(&packs, &id) {
            Ok(()) => {
                let _ = tx.send(AudioCmd::PackInstalled { index, ok: true });
            }
            Err(e) => {
                let _ = notices.send(Notice::DownloadFailed {
                    id: id.clone(),
                    error: e,
                });
                let _ = tx.send(AudioCmd::PackInstalled { index, ok: false });
            }
        });
    }

    fn is_cached(&self, index: usize) -> bool {
        match (self.profiles.get(index), self.packs_dir.as_deref()) {
            (Some(p), Some(dir)) => !p.is_optional() || download::is_installed(dir, &p.id),
            _ => false,
        }
    }

    fn warn(&self, message: String) {
        let _ = self.notices.send(Notice::Warn(message));
    }

    fn sound(&mut self, stream: &rodio::OutputStream, scan_code: u16) {
        if self.paused {
            return;
        }
        if let Some(pack) = self.pack.as_mut() {
            let clip = pack.clip_for(scan_code);
            play(stream, &clip, self.volume);
        }
    }

    fn preview(&mut self, stream: &rodio::OutputStream) {
        if let Some(pack) = self.pack.as_mut() {
            let clip = pack.next_default();
            play(stream, &clip, self.volume);
        }
    }
}

pub fn spawn(
    profiles: Vec<Profile>,
    packs_dir: Option<PathBuf>,
    selected: usize,
    volume: u8,
    notices: Sender<Notice>,
) -> Sender<AudioCmd> {
    let (tx, rx) = std::sync::mpsc::channel::<AudioCmd>();
    let tx_for_state = tx.clone();
    std::thread::spawn(move || {
        let mut stream = match OutputStreamBuilder::open_default_stream() {
            Ok(s) => s,
            Err(e) => {
                let _ = notices.send(Notice::Warn(format!("No audio output: {e}")));
                drain_commands(rx);
                return;
            }
        };
        stream.log_on_drop(false);

        let mut state = State::new(profiles, packs_dir, notices, tx_for_state, volume);
        state.select(selected, false);

        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(scan_code) => state.sound(&stream, scan_code),
                AudioCmd::Preview => state.preview(&stream),
                AudioCmd::SetProfile(i) => {
                    if state.select(i, true) {
                        state.preview(&stream);
                    }
                }
                AudioCmd::SetVolume(v) => state.volume = v,
                AudioCmd::SetPaused(p) => state.paused = p,
                AudioCmd::PackInstalled { index, ok } => {
                    if state.installed(index, ok) {
                        state.preview(&stream);
                    }
                }
            }
        }
    });
    tx
}
```

Update the existing `builtin_profile_loads` and `custom_profile_loads` calls from `load(&profile)` to `load(&profile, None)`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test audio::tests`
Expected: 6 tests pass (2 old + 4 new). `select_cached_optional_loads_and_previews` requires the `holy-panda` cache fixture; `state_fixture` installs it.

- [ ] **Step 6: Commit**

```bash
git add src/audio.rs src/profiles.rs
git commit -m "feat: download optional packs from the audio thread"
```

---

### Task 4: Main wiring and notices

**Files:**
- Modify: `src/main.rs`
- Test: compile check (`cargo check`)

- [ ] **Step 1: Build entries and pass the packs directory**

Replace the profile/setup block in `main`:

```rust
let user_dir = config::dir().map(|d| d.join("sounds"));
let packs_dir = config::dir().map(|d| d.join("packs"));
let (profiles_vec, warnings) = profiles::all(user_dir.as_deref());
let entries: Vec<ui::ProfileEntry> = profiles_vec
    .iter()
    .map(|p| ui::ProfileEntry {
        id: p.id.clone(),
        name: p.name.clone(),
        optional: p.is_optional(),
        available: p.is_available(packs_dir.as_deref()),
        downloading: false,
    })
    .collect();

let mut app = ui::new(entries, &cfg);
```

Change the audio spawn call:

```rust
let audio_tx = audio::spawn(
    profiles_vec,
    packs_dir,
    app.selected,
    app.volume,
    notice_tx.clone(),
);
```

- [ ] **Step 2: Run the compiler**

Run: `cargo check`
Expected: fails only because `ui::ProfileEntry` and the UI signatures are not updated yet (Task 5). If Task 5 is already merged, it passes.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire optional pack notices and cache directory"
```

---

### Task 5: UI markers and notices (design lane)

**Files:**
- Modify: `src/ui.rs`

**Design intent (do not flatten):**
- Optional-but-not-downloaded entries show a subtle download affordance at the end of the row. A `↓` glyph is sufficient; keep the existing `▸` selection marker leading.
- A download in progress shows a distinct busy suffix (`…`) and is visually distinguishable from "not downloaded".
- Available optional packs look identical to embedded packs.
- The profile list already uses `ListState`; 18 entries must scroll so the selected row stays visible. Do not add a second pane or change the six-row chrome.
- The notice line continues to carry one line of text. Downloading/completed text should not look like an error; failures stay in the existing yellow/red notice treatment.

- [ ] **Step 1: Replace the profile view model**

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileEntry {
    pub id: String,
    pub name: String,
    pub optional: bool,
    pub available: bool,
    pub downloading: bool,
}

#[derive(Clone)]
pub struct App {
    pub profiles: Vec<ProfileEntry>,
    pub selected: usize,
    pub volume: u8,
    pub paused: bool,
    pub listener_error: Option<String>,
    pub notice: Option<String>,
    pub quit: bool,
}
```

Update `new`, `config`, and `on_key` to use `entry.id` / `entry.name`.

- [ ] **Step 2: Handle the new notices**

```rust
Notice::Downloading(id) => {
    let name = self
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.clone());
    if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
        entry.downloading = true;
    }
    self.notice = Some(format!("Downloading {name}…"));
}
Notice::Downloaded(id) => {
    let name = self
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.clone());
    if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
        entry.available = true;
        entry.downloading = false;
    }
    self.notice = Some(format!("Downloaded {name}"));
}
Notice::DownloadFailed { id, error } => {
    let name = self
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| id.clone());
    if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
        entry.downloading = false;
    }
    self.notice = Some(format!("Could not download {name}: {error}"));
}
```

- [ ] **Step 3: Render the markers**

For each entry, build the label from the selection marker, name, and suffix:

```rust
let suffix = if entry.downloading {
    "  …"
} else if entry.optional && !entry.available {
    "  ↓"
} else {
    ""
};
```

Keep the selected style cyan/bold. Dim optional-unavailable rows (`Color::DarkGray`) and use yellow for downloading. Available rows use the default style.

- [ ] **Step 4: Stop sending the redundant preview**

In `run`, remove the `if is_set_profile { audio.send(AudioCmd::Preview) }` block. The audio thread now previews after a successful selection. Keep saving config on `SetProfile` and `SetVolume`.

- [ ] **Step 5: Update the tests**

Rebuild the `app()` fixture with four `ProfileEntry` values. Add:

```rust
#[test]
fn renders_optional_download_marker() {
    let mut a = app();
    a.profiles.push(ProfileEntry {
        id: "mx-black".into(),
        name: "Cherry MX Black (PBT)".into(),
        optional: true,
        available: false,
        downloading: false,
    });
    let backend = TestBackend::new(60, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &a)).unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Cherry MX Black (PBT)  ↓"), "{text}");
}

#[test]
fn download_notices_update_entry() {
    let mut a = app();
    a.profiles.push(ProfileEntry {
        id: "mx-black".into(),
        name: "Cherry MX Black (PBT)".into(),
        optional: true,
        available: false,
        downloading: false,
    });

    on_notice(&mut a, Notice::Downloading("mx-black".into()));
    assert!(a.profiles.last().unwrap().downloading);
    assert!(a.notice.as_ref().unwrap().contains("Downloading"));

    on_notice(&mut a, Notice::Downloaded("mx-black".into()));
    let entry = a.profiles.last().unwrap();
    assert!(entry.available);
    assert!(!entry.downloading);
    assert!(a.notice.as_ref().unwrap().contains("Downloaded"));
}

#[test]
fn download_failure_clears_busy_state() {
    let mut a = app();
    a.profiles.push(ProfileEntry {
        id: "mx-black".into(),
        name: "Cherry MX Black (PBT)".into(),
        optional: true,
        available: false,
        downloading: true,
    });
    on_notice(
        &mut a,
        Notice::DownloadFailed {
            id: "mx-black".into(),
            error: "offline".into(),
        },
    );
    assert!(!a.profiles.last().unwrap().downloading);
    assert!(a.notice.as_ref().unwrap().contains("offline"));
}

#[test]
fn scrolls_to_selected_optional() {
    let entries: Vec<ProfileEntry> = (0..18)
        .map(|i| ProfileEntry {
            id: format!("p{i}"),
            name: format!("Pack {i}"),
            optional: i >= 4,
            available: i < 4,
            downloading: false,
        })
        .collect();
    let mut a = new(entries, &Config::default());
    a.selected = 17;
    let backend = TestBackend::new(60, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &a)).unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Pack 17"), "{text}");
}
```

Replace `renders_all_builtin_profiles` with a test over the four embedded profiles. Keep the tiny-terminal test.

- [ ] **Step 6: Run the UI tests**

Run: `cargo test ui::tests`
Expected: all UI tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs
git commit -m "feat: show optional pack download state in the profile list"
```

---

## Chunk 2: Assets

### Task 6: Split the generator

**Files:**
- Modify: `scripts/build_sound_packs.py`

- [ ] **Step 1: Replace the output root and package map**

Replace lines 26-41 with:

```python
OUT_DEFAULT = ROOT / "assets" / "sounds"
OUT_OPTIONAL = ROOT / "assets" / "sounds-optional"

DEFAULT_PACKS = {
    "mx-blue": "cherrymx-blue-abs",
    "mx-red": "cherrymx-red-abs",
    "mx-brown": "cherrymx-brown-abs",
    "topre": "topre-purple-hybrid-pbt",
}

OPTIONAL_PACKS = {
    "mx-black": "cherrymx-black-pbt",
    "mx-black-abs": "cherrymx-black-abs",
    "mx-blue-pbt": "cherrymx-blue-pbt",
    "mx-red-pbt": "cherrymx-red-pbt",
    "mx-brown-pbt": "cherrymx-brown-pbt",
    "holy-panda": "holy-pandas",
    "nk-cream": "nk-cream",
    "eg-oreo": "eg-oreo",
    "eg-crystal-purple": "eg-crystal-purple",
    "turquoise": "turquoise",
    "cream-travel": "cream-travel",
    "mx-black-travel": "mxblack-travel",
    "mx-blue-travel": "mxblue-travel",
    "mx-brown-travel": "mxbrown-travel",
}

PACKS = {**DEFAULT_PACKS, **OPTIONAL_PACKS}
```

- [ ] **Step 2: Add the output-root parameter**

Change `build(profile, source_dir)` to `build(profile: str, source_dir: str, out_root: Path)`, and change `dest_dir = OUT_ROOT / profile` to `dest_dir = out_root / profile`.

- [ ] **Step 3: Update the CLI**

Replace `main()`:

```python
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--only",
        action="append",
        choices=sorted(PACKS),
        help="build one profile (repeatable)",
    )
    parser.add_argument(
        "--set",
        choices=["default", "optional"],
        help="limit a bulk build to one set",
    )
    args = parser.parse_args()

    if args.set == "default":
        selected = dict(DEFAULT_PACKS)
    elif args.set == "optional":
        selected = dict(OPTIONAL_PACKS)
    else:
        selected = dict(PACKS)

    if args.only:
        for profile in args.only:
            if profile not in selected:
                parser.error(f"{profile} is not in the selected set")
        selected = {profile: PACKS[profile] for profile in args.only}

    total_audio = 0
    total_manifest = 0
    for profile, source_dir in selected.items():
        out_root = OUT_OPTIONAL if profile in OPTIONAL_PACKS else OUT_DEFAULT
        audio, manifest = build(profile, source_dir, out_root)
        total_audio += audio
        total_manifest += manifest
        print(
            f"{profile:20} {audio / 1024:8.1f} KiB flac  "
            f"{manifest:6d} B manifest"
        )
    print(
        f"{'total':20} {total_audio / 1024:8.1f} KiB flac  "
        f"{total_manifest:6d} B manifest"
    )
    return 0
```

Also update the docstring usage line to mention `--set` and repeatable `--only`.

- [ ] **Step 4: Verify the CLI without writing assets**

Run: `python3 scripts/build_sound_packs.py --help`
Expected: help shows `--only` and `--set {default,optional}`.

Run: `python3 scripts/build_sound_packs.py --only mx-blue --set optional 2>&1 | tail -3`
Expected: `error: mx-blue is not in the selected set`.

- [ ] **Step 5: Commit**

```bash
git add scripts/build_sound_packs.py
git commit -m "feat: split sound pack generator into default and optional sets"
```

---

### Task 7: Move and generate assets

**Files:**
- Move: `assets/sounds/{mx-black,mx-black-abs,holy-panda,nk-cream,eg-oreo,eg-crystal-purple,turquoise}` → `assets/sounds-optional/`
- Create: `assets/sounds-optional/{mx-blue-pbt,mx-red-pbt,mx-brown-pbt,cream-travel,mx-black-travel,mx-blue-travel,mx-brown-travel}/`
- Move: `assets/sounds/CREDITS.md` → `assets/CREDITS.md`
- Modify: `assets/CREDITS.md`

- [ ] **Step 1: Move the seven existing optional packs**

```bash
mkdir -p assets/sounds-optional
for id in mx-black mx-black-abs holy-panda nk-cream eg-oreo eg-crystal-purple turquoise; do
  git mv "assets/sounds/$id" "assets/sounds-optional/$id"
done
git mv assets/sounds/CREDITS.md assets/CREDITS.md
```

- [ ] **Step 2: Generate the seven new packs**

```bash
python3 scripts/build_sound_packs.py \
  --only mx-blue-pbt \
  --only mx-red-pbt \
  --only mx-brown-pbt \
  --only cream-travel \
  --only mx-black-travel \
  --only mx-blue-travel \
  --only mx-brown-travel
```

Expected: seven lines plus a total; no errors. Each output lands in `assets/sounds-optional/<id>/`.

- [ ] **Step 3: Verify the layout and decode every optional pack**

```bash
find assets/sounds -mindepth 1 -maxdepth 1 -type d | sort
find assets/sounds-optional -mindepth 1 -maxdepth 1 -type d | sort
for d in assets/sounds-optional/*/; do
  python3 - <<'PY' "$d"
import json, sys
from pathlib import Path
p = Path(sys.argv[1])
m = json.loads((p / "config.json").read_text())
assert m["version"] == 1 and m["channels"] == 1
assert m["keys"] and m["defaults"]
print(f"{p.name}: {len(m['keys'])} keys, {len(m['defaults'])} defaults, {m['total_samples']} samples")
PY
done
```

Expected: `assets/sounds` has exactly 4 directories; `assets/sounds-optional` has exactly 14; every optional manifest has keys/defaults.

- [ ] **Step 4: Add the optional-assets decode test**

In `src/profiles.rs` tests, add:

```rust
#[test]
fn optional_assets_decode_from_disk() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/sounds-optional");
    let mut ids: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    assert_eq!(ids.len(), 14, "{ids:?}");
    for id in ids {
        let dir = root.join(&id);
        let manifest = std::fs::read(dir.join("config.json")).unwrap();
        let sprite = std::fs::read(dir.join("sound.flac")).unwrap();
        let pack = LoadedPack::from_builtin(&sprite, &manifest)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(!pack.keys.is_empty(), "{id} has no keys");
        assert!(!pack.defaults.is_empty(), "{id} has no defaults");
    }
}
```

- [ ] **Step 5: Update `assets/CREDITS.md`**

Keep the format/fade/normalization sections. Change the intro to say "Every file in `assets/sounds/` and `assets/sounds-optional/`", and replace the source table with a Default/Optional column. Required rows:

```markdown
| Directory | Set | Switch | Source pack | Author | License |
|---|---|---|---|---|---|
| `mx-blue` | default | Cherry MX Blue (ABS) | [`cherrymx-blue-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-blue-abs) | Connor Langan / MechVibes | MIT |
| `mx-red` | default | Cherry MX Red (ABS) | [`cherrymx-red-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-red-abs) | Connor Langan / MechVibes | MIT |
| `mx-brown` | default | Cherry MX Brown (ABS) | [`cherrymx-brown-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-brown-abs) | Connor Langan / MechVibes | MIT |
| `topre` | default | Topre Purple Hybrid (PBT) | [`topre-purple-hybrid-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/topre-purple-hybrid-pbt) | Connor Langan / MechVibes | MIT |
| `mx-black` | optional | Cherry MX Black (PBT) | [`cherrymx-black-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-black-pbt) | Connor Langan / MechVibes | MIT |
| `mx-black-abs` | optional | Cherry MX Black (ABS) | [`cherrymx-black-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-black-abs) | Connor Langan / MechVibes | MIT |
| `mx-blue-pbt` | optional | Cherry MX Blue (PBT) | [`cherrymx-blue-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-blue-pbt) | Connor Langan / MechVibes | MIT |
| `mx-red-pbt` | optional | Cherry MX Red (PBT) | [`cherrymx-red-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-red-pbt) | Connor Langan / MechVibes | MIT |
| `mx-brown-pbt` | optional | Cherry MX Brown (PBT) | [`cherrymx-brown-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-brown-pbt) | Connor Langan / MechVibes | MIT |
| `holy-panda` | optional | Holy Panda | [`holy-pandas`](https://github.com/cjlangan/MechSim/tree/main/audio/holy-pandas) | Connor Langan / MechVibes | MIT |
| `nk-cream` | optional | NK Cream | [`nk-cream`](https://github.com/cjlangan/MechSim/tree/main/audio/nk-cream) | Connor Langan / MechVibes | MIT |
| `eg-oreo` | optional | Everglide Oreo | [`eg-oreo`](https://github.com/cjlangan/MechSim/tree/main/audio/eg-oreo) | Connor Langan / MechVibes | MIT |
| `eg-crystal-purple` | optional | Everglide Crystal Purple | [`eg-crystal-purple`](https://github.com/cjlangan/MechSim/tree/main/audio/eg-crystal-purple) | Connor Langan / MechVibes | MIT |
| `turquoise` | optional | Turquoise Tealios | [`turquoise`](https://github.com/cjlangan/MechSim/tree/main/audio/turquoise) | Connor Langan / MechVibes | MIT |
| `cream-travel` | optional | Cream (Full Travel) | [`cream-travel`](https://github.com/cjlangan/MechSim/tree/main/audio/cream-travel) | Connor Langan / MechVibes | MIT |
| `mx-black-travel` | optional | Cherry MX Black (Full Travel) | [`mxblack-travel`](https://github.com/cjlangan/MechSim/tree/main/audio/mxblack-travel) | Connor Langan / MechVibes | MIT |
| `mx-blue-travel` | optional | Cherry MX Blue (Full Travel) | [`mxblue-travel`](https://github.com/cjlangan/MechSim/tree/main/audio/mxblue-travel) | Connor Langan / MechVibes | MIT |
| `mx-brown-travel` | optional | Cherry MX Brown (Full Travel) | [`mxbrown-travel`](https://github.com/cjlangan/MechSim/tree/main/audio/mxbrown-travel) | Connor Langan / MechVibes | MIT |
```

Also add one sentence after the table: "Optional packs are the same FLAC format as the defaults; brokey downloads them from this repository on first selection and caches them under the user config directory." Keep the MIT notice unchanged.

- [ ] **Step 6: Check sizes and commit**

```bash
du -sh assets/sounds assets/sounds-optional
git add assets
git commit -m "feat: move optional packs and add seven MIT recordings"
```

Expected: `assets/sounds` about 1.6 MB, `assets/sounds-optional` about 3 MB.

---

## Chunk 3: Documentation and verification

### Task 8: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the built-in profiles section**

Replace `## Built-in profiles` (lines 53-70) with:

```markdown
## Profiles

### Default profiles (embedded)

These are compiled into the binary and work with no network:

- `mx-blue` "Cherry MX Blue (ABS)"
- `mx-red` "Cherry MX Red (ABS)"
- `mx-brown` "Cherry MX Brown (ABS)"
- `topre` "Topre"

### Optional profiles (downloaded on first use)

These appear in the same list. Selecting one downloads it from this
repository's `assets/sounds-optional/` directory and caches it at
`<config dir>/brokey/packs/<id>/`. Rows that are not downloaded show a `↓`
marker; a download in progress shows `…`. Until a download finishes, the
previous pack keeps playing.

- `mx-black` "Cherry MX Black (PBT)"
- `mx-black-abs` "Cherry MX Black (ABS)"
- `mx-blue-pbt` "Cherry MX Blue (PBT)"
- `mx-red-pbt` "Cherry MX Red (PBT)"
- `mx-brown-pbt` "Cherry MX Brown (PBT)"
- `holy-panda` "Holy Panda"
- `nk-cream` "NK Cream"
- `eg-oreo` "Everglide Oreo"
- `eg-crystal-purple` "Everglide Crystal Purple"
- `turquoise` "Turquoise Tealios"
- `cream-travel` "Cream (Full Travel)"
- `mx-black-travel` "Cherry MX Black (Full Travel)"
- `mx-blue-travel` "Cherry MX Blue (Full Travel)"
- `mx-brown-travel` "Cherry MX Brown (Full Travel)"

Embedded and optional packs use real per-key recordings: letters, digits,
common punctuation, Space, Enter, Backspace, and Tab each have their own sample
where the source pack provides one. Keys without a recording, and the preview
key, rotate through natural fallback takes from the same pack. Full-travel
packs only have per-key samples for Backspace, Enter, and Space; the rest use
their generic takes.
```

- [ ] **Step 2: Update the keys table note and credits link**

Change the two "Select profile (switching also previews it)" rows to
"Select profile (downloads an optional pack first, then previews it)".

Change the credits sentence at the bottom to:

```markdown
The bundled and optional sounds are real recordings from MIT-licensed packs;
see `assets/CREDITS.md` for provenance and attribution.
```

- [ ] **Step 3: Update privacy**

Replace the final privacy paragraph with:

```markdown
Brokey uses the keycode only to choose a sound; it does not build, log, or store
text. No audio is recorded. The network is used only when an optional profile is
selected for the first time; brokey then requests that pack's `config.json` and
`sound.flac` from this repository and nothing else. There is no telemetry.
```

- [ ] **Step 4: Verify no stale references**

Run: `grep -n "11\|assets/sounds/CREDITS\|Built-in profiles" README.md`
Expected: no stale "11 profiles" or old credits path.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document default and optional sound packs"
```

---

### Task 9: Final verification

**Files:** all

- [ ] **Step 1: Format and lint**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Expected: both pass with no warnings. If `cargo fmt --check` fails, run `cargo fmt` and amend the relevant commit.

- [ ] **Step 2: Full test suite**

```bash
cargo test
```

Expected: all tests pass (baseline 40, adjusted for the new catalog/UI/download tests).

- [ ] **Step 3: Release build and size check**

```bash
cargo build --release
ls -lh target/release/brokey
du -sh assets/sounds assets/sounds-optional
```

Expected: binary no larger than the 6.5 MB baseline (ureq/rustls may offset part of the ~1.5 MB asset reduction), `assets/sounds` ~1.6 MB, `assets/sounds-optional` ~3 MB.

- [ ] **Step 4: Raw URL check (after pushing)**

The download URL only works once the assets are on `origin/master`. After the
user pushes, run:

```bash
curl -fsSI https://raw.githubusercontent.com/ntk148v/brokey/master/assets/sounds-optional/mx-blue-pbt/config.json | head -1
curl -fsSI https://raw.githubusercontent.com/ntk148v/brokey/master/assets/sounds-optional/mx-blue-pbt/sound.flac | head -1
```

Expected: `HTTP/2 200` for both. If the repo is not pushed yet, record this as
the only unverified acceptance item.

- [ ] **Step 5: Manual smoke test**

Run `cargo run --release` and check:

1. With the cache empty, the four default profiles load and play offline.
2. Selecting `mx-black` (or another optional) shows `…`, then `Downloaded`,
   then plays a preview; typing plays the new pack.
3. Quit and restart: the optional pack loads from cache without network.
4. Delete `<config>/brokey/packs/<id>/`, select it again: it re-downloads.
5. Disable network, select an uncached optional: the notice shows the failure,
   the previous pack still plays, and the app does not hang.
6. Custom packs in `<config>/brokey/sounds/` still load.
7. Pause, volume, preview, and quit still work; switching profiles previews
   while paused.

- [ ] **Step 6: Final commit if anything remains**

```bash
git status --short
git add -A
git commit -m "chore: finish optional pack verification"
```

Do not commit `.claude/` or `docs/spec.md`; they were untracked before this
work.

---

## Notes for the implementer

- Do not regenerate the seven moved packs; the committed FLACs are the source
  of truth.
- Do not add a new keybinding. Selection starts the download.
- Keep `AudioCmd` `Copy`; the worker sends only `index`/`ok`, and the audio
  thread reloads from cache.
- The only network calls are the two `GET`s in `download::fetch`.
- If the independent spec/plan reviewers are unavailable, self-review against
  the spec and record the limitation in the final summary.
