# Natural per-key typing sounds Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace each built-in profile's single press clip with real per-key recordings from MechVibes/MechSim, add the requested Cherry MX Black ABS pack, and keep the committed audio under 4 MB.

**Architecture:** A Python maintenance script downloads MechSim packs, slices the 51 typing keys, normalizes/fades them, and writes one trimmed FLAC sprite plus a JSON manifest per profile. At runtime only the selected pack is decoded into memory; the listener maps `rdev::Key` to MechVibes scan codes and the audio thread plays the matching slice or rotates through fallback takes. Custom `press.*`/`space.*` packs keep working through the same in-memory pack type.

**Tech Stack:** Rust 2021, rodio 0.21.1, rdev 0.5.3, serde/serde_json, Python 3 standard library + ffmpeg (asset generation only).

**Spec:** `docs/design/2026-10-28-natural-key-sounds-design.md`

---

## Notes for the implementer

- Run every command from the repository root.
- Do not change the custom-pack format: `<config>/brokey/sounds/<name>/press.<ext>` plus optional `space.<ext>`.
- Do not log keycodes or typed text anywhere.
- Keep `mx-black-abs` last in `builtins()` and in the README list.
- Old `press.wav`/`space.wav` files stay committed until Task 6 so every intermediate `cargo test` is green.
- If a download fails, fix the URL and retry. Never commit a partially generated `assets/sounds/<id>/`.

---

## Chunk 1: Sound assets

### Task 1: Add the pack generator

**Files:**
- Create: `scripts/build_sound_packs.py`

- [ ] **Step 1: Write the generator**

Create `scripts/build_sound_packs.py` with this exact content:

```python
#!/usr/bin/env python3
"""Build brokey's trimmed per-key sound packs from MechSim recordings.

This is a maintenance tool. The generated `sound.flac` and `config.json`
files are committed, so normal builds and runtime need neither network access
nor ffmpeg.

Usage:
    python3 scripts/build_sound_packs.py [--only ID]
"""

from __future__ import annotations

import argparse
import array
import json
import re
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

RAW = "https://raw.githubusercontent.com/cjlangan/MechSim/main/audio"
ROOT = Path(__file__).resolve().parent.parent
OUT_ROOT = ROOT / "assets" / "sounds"

PACKS = {
    "mx-blue": "cherrymx-blue-abs",
    "mx-red": "cherrymx-red-abs",
    "mx-brown": "cherrymx-brown-abs",
    "topre": "topre-purple-hybrid-pbt",
    "mx-black": "cherrymx-black-pbt",
    "mx-black-abs": "cherrymx-black-abs",
    "holy-panda": "holy-pandas",
    "nk-cream": "nk-cream",
    "eg-oreo": "eg-oreo",
    "eg-crystal-purple": "eg-crystal-purple",
    "turquoise": "turquoise",
}

# MechVibes scan codes for the keys people actually type on.
KEYS = sorted(
    {
        2, 3, 4, 5, 6, 7, 8, 9, 10, 11,       # 1..0
        12, 13,                                # - =
        16, 17, 18, 19, 20, 21, 22, 23, 24, 25,  # Q..P
        26, 27,                                # [ ]
        30, 31, 32, 33, 34, 35, 36, 37, 38,   # A..L
        39, 40, 41, 43,                        # ; ' ` backslash
        44, 45, 46, 47, 48, 49, 50,           # Z..M
        51, 52, 53,                            # , . /
        14, 15, 28, 57,                        # Backspace Tab Enter Space
    }
)

RATE = 44100
FADE_IN_SAMPLES = int(RATE * 0.002)
FADE_OUT_SAMPLES = int(RATE * 0.006)
PEAK = 10 ** (-2 / 20)  # -2 dBFS
DEFAULT_CODES = [30, 31, 32, 33, 34]  # A S D F G


def fetch(url: str) -> bytes:
    try:
        with urllib.request.urlopen(url, timeout=120) as resp:
            return resp.read()
    except Exception as exc:
        raise RuntimeError(f"download failed: {url}: {exc}") from exc


def download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(fetch(url))


def ffmpeg_pcm(
    path: Path, start: float | None = None, duration: float | None = None
) -> list[float]:
    cmd = ["ffmpeg", "-v", "error", "-nostdin"]
    if start is not None:
        cmd += ["-ss", f"{start:.6f}"]
    cmd += ["-i", str(path)]
    if duration is not None:
        cmd += ["-t", f"{duration:.6f}"]
    cmd += [
        "-f",
        "f32le",
        "-acodec",
        "pcm_f32le",
        "-ac",
        "1",
        "-ar",
        str(RATE),
        "-",
    ]
    proc = subprocess.run(cmd, capture_output=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"ffmpeg failed for {path}: "
            f"{proc.stderr.decode(errors='replace').strip()}"
        )
    samples = array.array("f")
    samples.frombytes(proc.stdout[: len(proc.stdout) // 4 * 4])
    if sys.byteorder != "little":
        samples.byteswap()
    return list(samples)


def apply_fades(clip: list[float]) -> list[float]:
    n = len(clip)
    fade_in = min(FADE_IN_SAMPLES, n // 2)
    fade_out = min(FADE_OUT_SAMPLES, n // 2)
    for i in range(fade_in):
        clip[i] *= (i + 1) / (fade_in + 1)
    for i in range(fade_out):
        clip[n - 1 - i] *= (i + 1) / (fade_out + 1)
    return clip


def encode_flac(samples: list[float], dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    raw = array.array("f", samples)
    if sys.byteorder != "little":
        raw.byteswap()
    cmd = [
        "ffmpeg",
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-f",
        "f32le",
        "-ac",
        "1",
        "-ar",
        str(RATE),
        "-i",
        "pipe:0",
        "-c:a",
        "flac",
        "-compression_level",
        "8",
        "-sample_fmt",
        "s16",
        "-map_metadata",
        "-1",
        "-fflags",
        "+bitexact",
        str(dest),
    ]
    proc = subprocess.run(cmd, input=raw.tobytes(), capture_output=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"flac encode failed: "
            f"{proc.stderr.decode(errors='replace').strip()}"
        )


def url_for(source_dir: str, rel: str) -> str:
    return f"{RAW}/{source_dir}/{urllib.parse.quote(rel, safe='/')}"


def build(profile: str, source_dir: str) -> tuple[int, int]:
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        cfg = json.loads(fetch(f"{RAW}/{source_dir}/config.json"))
        kind = cfg.get("key_define_type", "single")
        defines = cfg.get("defines", {})
        sound = cfg.get("sound", "sound.ogg")

        sprite = None
        if kind == "single":
            sprite = tmp / "sound.ogg"
            download(url_for(source_dir, sound), sprite)

        buf: list[float] = []
        keys: dict[int, tuple[int, int]] = {}

        for code in KEYS:
            entry = defines.get(str(code))
            if entry is None:
                continue
            if kind == "single":
                start_ms, duration_ms = entry
                clip = ffmpeg_pcm(sprite, start_ms / 1000.0, duration_ms / 1000.0)
            else:
                rel = entry
                path = tmp / f"{code}-{Path(rel).name}"
                download(url_for(source_dir, rel), path)
                clip = ffmpeg_pcm(path)
            clip = apply_fades(clip)
            if not clip:
                continue
            keys[code] = (len(buf), len(clip))
            buf.extend(clip)

        if not buf:
            raise RuntimeError(f"{profile}: no key clips found")

        defaults: list[tuple[int, int]] = []
        if kind == "multi":
            match = re.search(r"\{(\d+)-(\d+)\}", sound)
            if match:
                lo, hi = int(match.group(1)), int(match.group(2))
                for i in range(lo, hi + 1):
                    rel = sound[: match.start()] + str(i) + sound[match.end() :]
                    path = tmp / f"default-{i}-{Path(rel).name}"
                    download(url_for(source_dir, rel), path)
                    clip = apply_fades(ffmpeg_pcm(path))
                    if clip:
                        defaults.append((len(buf), len(clip)))
                        buf.extend(clip)

        if not defaults:
            for code in DEFAULT_CODES:
                if code in keys:
                    defaults.append(keys[code])
        if not defaults:
            defaults = [range_ for _, range_ in sorted(keys.items())[:5]]

        peak = max((abs(sample) for sample in buf), default=0.0)
        if peak > 0.0:
            gain = PEAK / peak
            buf = [sample * gain for sample in buf]

        dest_dir = OUT_ROOT / profile
        encode_flac(buf, dest_dir / "sound.flac")
        manifest = {
            "version": 1,
            "profile": profile,
            "source": {
                "repo": "https://github.com/cjlangan/MechSim",
                "dir": f"audio/{source_dir}",
                "license": "MIT",
            },
            "sample_rate": RATE,
            "channels": 1,
            "total_samples": len(buf),
            "audio": "sound.flac",
            "keys": {str(code): list(range_) for code, range_ in sorted(keys.items())},
            "defaults": [list(range_) for range_ in defaults],
        }
        (dest_dir / "config.json").write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )

        return (
            (dest_dir / "sound.flac").stat().st_size,
            (dest_dir / "config.json").stat().st_size,
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--only", choices=sorted(PACKS), help="build one profile")
    args = parser.parse_args()
    profiles = [args.only] if args.only else list(PACKS)
    total_audio = 0
    total_manifest = 0
    for profile in profiles:
        audio, manifest = build(profile, PACKS[profile])
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


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Check the script parses**

Run:

```bash
python3 -m py_compile scripts/build_sound_packs.py
```

Expected: no output, exit code 0.

- [ ] **Step 3: Generate one pack as a smoke test**

Run:

```bash
python3 scripts/build_sound_packs.py --only mx-black-abs
```

Expected: one line like `mx-black-abs          266.1 KiB flac    2710 B manifest`.

Verify:

```bash
python3 - <<'PY'
import json
m = json.load(open("assets/sounds/mx-black-abs/config.json"))
assert m["version"] == 1
assert m["sample_rate"] == 44100
assert m["channels"] == 1
assert m["total_samples"] > 200000
assert "16" in m["keys"] and "57" in m["keys"]
assert m["defaults"]
print("ok", m["total_samples"], len(m["keys"]), len(m["defaults"]))
PY
```

Expected: `ok` followed by three numbers.

- [ ] **Step 4: Commit the generator**

```bash
git add scripts/build_sound_packs.py
git commit -m "feat: add per-key sound pack generator"
```

---

### Task 2: Generate and commit all 11 packs

**Files:**
- Create: `assets/sounds/<id>/sound.flac`
- Create: `assets/sounds/<id>/config.json`
- Keep: `assets/sounds/<id>/press.wav`, `space.wav` until Task 6

- [ ] **Step 1: Generate every pack**

Run:

```bash
python3 scripts/build_sound_packs.py
```

Expected: 11 profile lines plus a total line. This downloads ~30 MB of source
audio and may take a few minutes.

- [ ] **Step 2: Verify the generated assets**

Run:

```bash
test "$(find assets/sounds -name sound.flac | wc -l | tr -d ' ')" = 11
find assets/sounds -name sound.flac -exec du -ch {} + | tail -1
for f in assets/sounds/*/config.json; do
  python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$f" || exit 1
done
```

Expected: the first command prints nothing and exits 0; the `du` total is under
4.0M; every manifest parses.

Also confirm the requested pack exists and has per-key data:

```bash
python3 - <<'PY'
import json
m = json.load(open("assets/sounds/mx-black-abs/config.json"))
assert m["source"]["dir"] == "audio/cherrymx-black-abs"
assert len(m["keys"]) >= 50
print("mx-black-abs keys:", len(m["keys"]))
PY
```

Expected: `mx-black-abs keys:` followed by a number >= 50.

- [ ] **Step 3: Run the existing test suite**

Run:

```bash
cargo test
```

Expected: all tests pass. The old code still reads `press.wav`/`space.wav`, which
are intentionally still present.

- [ ] **Step 4: Commit the assets**

```bash
git add assets/sounds
git commit -m "feat: bundle per-key sound assets"
```

---

## Chunk 2: Manifest and playback primitives

### Task 3: Add `src/pack.rs`

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Create: `src/pack.rs`
- Test: `src/pack.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Add the dependency and module**

In `Cargo.toml`, add `serde_json` under `[dependencies]`:

```toml
serde_json = "1"
```

In `src/main.rs`, add the module next to the existing ones:

```rust
pub mod pack;
```

- [ ] **Step 2: Write the failing tests**

Create `src/pack.rs` containing only this test module (the implementation comes
next, so it will not compile yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Manifest, String> {
        Manifest::parse(json.as_bytes())
    }

    const VALID: &str = r#"{
        "version": 1,
        "profile": "test",
        "sample_rate": 44100,
        "channels": 1,
        "total_samples": 10,
        "audio": "sound.flac",
        "keys": {"16": [0, 5], "57": [5, 5]},
        "defaults": [[0, 5]]
    }"#;

    #[test]
    fn parses_valid_manifest() {
        let m = parse(VALID).unwrap();
        assert_eq!(m.sample_rate, 44100);
        assert_eq!(m.channels, 1);
        assert_eq!(m.total_samples, 10);
        assert_eq!(m.keys.get(&16), Some(&(0, 5)));
        assert_eq!(m.keys.get(&57), Some(&(5, 5)));
        assert_eq!(m.defaults, vec![(0, 5)]);
    }

    #[test]
    fn rejects_wrong_version() {
        let m = VALID.replace("\"version\": 1", "\"version\": 2");
        assert!(parse(&m).unwrap_err().contains("version"));
    }

    #[test]
    fn rejects_out_of_range() {
        let m = VALID.replace("\"57\": [5, 5]", "\"57\": [8, 5]");
        assert!(parse(&m).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn rejects_zero_length() {
        let m = VALID.replace("\"16\": [0, 5]", "\"16\": [0, 0]");
        assert!(parse(&m).unwrap_err().contains("zero length"));
    }

    #[test]
    fn rejects_empty_defaults() {
        let m = VALID.replace("\"defaults\": [[0, 5]]", "\"defaults\": []");
        assert!(parse(&m).unwrap_err().contains("default"));
    }

    #[test]
    fn rejects_stereo() {
        let m = VALID.replace("\"channels\": 1", "\"channels\": 2");
        assert!(parse(&m).unwrap_err().contains("mono"));
    }

    #[test]
    fn rejects_bad_scan_code() {
        let m = VALID.replace("\"16\": [0, 5]", "\"x\": [0, 5]");
        assert!(parse(&m).unwrap_err().contains("scan code"));
    }

    fn clip(start: usize) -> PlaybackClip {
        PlaybackClip {
            samples: Arc::new(vec![0.0; 8]),
            channels: 1,
            rate: 44100,
            start,
            len: 4,
        }
    }

    #[test]
    fn falls_back_and_rotates() {
        let mut pack = LoadedPack {
            keys: HashMap::new(),
            defaults: vec![clip(0), clip(4)],
            next_default: 0,
        };
        assert_eq!(pack.clip_for(99).start, 0);
        assert_eq!(pack.clip_for(99).start, 4);
        assert_eq!(pack.clip_for(99).start, 0);
    }

    #[test]
    fn prefers_specific_key() {
        let mut pack = LoadedPack {
            keys: HashMap::new(),
            defaults: vec![clip(0)],
            next_default: 0,
        };
        pack.keys.insert(16, clip(4));
        assert_eq!(pack.clip_for(16).start, 4);
        assert_eq!(pack.clip_for(17).start, 0);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run:

```bash
cargo test pack::
```

Expected: compile error mentioning `Manifest`/`PlaybackClip`/`LoadedPack` not
found.

- [ ] **Step 4: Implement the module**

Prepend this implementation above the test module in `src/pack.rs`:

```rust
use rodio::{Decoder, Source};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub sample_rate: u32,
    pub channels: u16,
    pub total_samples: usize,
    pub keys: HashMap<u16, (usize, usize)>,
    pub defaults: Vec<(usize, usize)>,
}

#[derive(Deserialize)]
struct RawManifest {
    version: u32,
    sample_rate: u32,
    channels: u16,
    total_samples: usize,
    keys: HashMap<String, [usize; 2]>,
    defaults: Vec<[usize; 2]>,
}

fn check_range(start: usize, len: usize, total: usize) -> Result<(), String> {
    if len == 0 {
        return Err("clip has zero length".to_string());
    }
    match start.checked_add(len) {
        Some(end) if end <= total => Ok(()),
        _ => Err(format!(
            "clip [{start}, {len}] exceeds total_samples {total}"
        )),
    }
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
        let raw: RawManifest =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid manifest: {e}"))?;
        if raw.version != 1 {
            return Err(format!("unsupported manifest version {}", raw.version));
        }
        if raw.sample_rate == 0 {
            return Err("manifest has zero sample rate".to_string());
        }
        if raw.channels != 1 {
            return Err(format!(
                "expected mono built-in, found {} channels",
                raw.channels
            ));
        }
        if raw.total_samples == 0 {
            return Err("manifest has no samples".to_string());
        }
        if raw.defaults.is_empty() {
            return Err("manifest has no default clips".to_string());
        }

        let mut keys = HashMap::with_capacity(raw.keys.len());
        for (code, [start, len]) in raw.keys {
            let code: u16 = code
                .parse()
                .map_err(|_| format!("invalid scan code '{code}'"))?;
            check_range(start, len, raw.total_samples)?;
            keys.insert(code, (start, len));
        }

        let mut defaults = Vec::with_capacity(raw.defaults.len());
        for [start, len] in raw.defaults {
            check_range(start, len, raw.total_samples)?;
            defaults.push((start, len));
        }

        Ok(Manifest {
            sample_rate: raw.sample_rate,
            channels: raw.channels,
            total_samples: raw.total_samples,
            keys,
            defaults,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PlaybackClip {
    pub samples: Arc<Vec<f32>>,
    pub channels: u16,
    pub rate: u32,
    pub start: usize,
    pub len: usize,
}

impl PlaybackClip {
    pub fn data(&self) -> Vec<f32> {
        self.samples[self.start..self.start + self.len].to_vec()
    }
}

pub struct LoadedPack {
    pub keys: HashMap<u16, PlaybackClip>,
    pub defaults: Vec<PlaybackClip>,
    next_default: usize,
}

impl LoadedPack {
    pub fn from_builtin(sprite: &'static [u8], manifest: &[u8]) -> Result<LoadedPack, String> {
        let manifest = Manifest::parse(manifest)?;
        let decoder =
            Decoder::new(Cursor::new(sprite)).map_err(|e| format!("could not decode FLAC: {e}"))?;
        let rate = decoder.sample_rate();
        let channels = decoder.channels();
        if rate != manifest.sample_rate {
            return Err(format!(
                "manifest sample rate {} does not match decoded {rate}",
                manifest.sample_rate
            ));
        }
        if channels != manifest.channels {
            return Err(format!(
                "manifest has {} channels but decoded {channels}",
                manifest.channels
            ));
        }
        let samples: Vec<f32> = decoder.collect();
        if samples.len() != manifest.total_samples {
            return Err(format!(
                "manifest total_samples {} does not match decoded {}",
                manifest.total_samples,
                samples.len()
            ));
        }

        let samples = Arc::new(samples);
        let keys = manifest
            .keys
            .into_iter()
            .map(|(code, (start, len))| {
                (
                    code,
                    PlaybackClip {
                        samples: Arc::clone(&samples),
                        channels,
                        rate,
                        start,
                        len,
                    },
                )
            })
            .collect();
        let defaults = manifest
            .defaults
            .into_iter()
            .map(|(start, len)| PlaybackClip {
                samples: Arc::clone(&samples),
                channels,
                rate,
                start,
                len,
            })
            .collect();

        Ok(LoadedPack {
            keys,
            defaults,
            next_default: 0,
        })
    }

    pub fn from_custom(press: Vec<u8>, space: Option<Vec<u8>>) -> Result<LoadedPack, String> {
        let press = decode_clip(press).map_err(|e| format!("press: {e}"))?;
        let mut keys = HashMap::new();
        if let Some(bytes) = space {
            if let Ok(clip) = decode_clip(bytes) {
                keys.insert(57, clip);
            }
        }
        Ok(LoadedPack {
            keys,
            defaults: vec![press],
            next_default: 0,
        })
    }

    pub fn clip_for(&mut self, scan_code: u16) -> PlaybackClip {
        if let Some(clip) = self.keys.get(&scan_code) {
            return clip.clone();
        }
        self.next_default()
    }

    pub fn next_default(&mut self) -> PlaybackClip {
        let clip = self.defaults[self.next_default % self.defaults.len()].clone();
        self.next_default = (self.next_default + 1) % self.defaults.len();
        clip
    }
}

fn decode_clip(bytes: Vec<u8>) -> Result<PlaybackClip, String> {
    let decoder = Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let rate = decoder.sample_rate();
    let channels = decoder.channels();
    if rate == 0 || channels == 0 {
        return Err("decoded clip has no rate or channels".to_string());
    }
    let samples: Vec<f32> = decoder.collect();
    if samples.is_empty() {
        return Err("decoded clip has no samples".to_string());
    }
    let len = samples.len();
    Ok(PlaybackClip {
        samples: Arc::new(samples),
        channels,
        rate,
        start: 0,
        len,
    })
}
```

- [ ] **Step 5: Run the tests**

Run:

```bash
cargo test pack::
```

Expected: all `pack::tests` pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/pack.rs
git commit -m "feat: parse sound manifests and playback clips"
```

---

## Chunk 3: Runtime switch to scan codes and lazy packs

### Task 4: Replace single clips with per-key packs

**Files:**
- Modify: `src/listener.rs`
- Modify: `src/profiles.rs`
- Modify: `src/audio.rs`
- Modify: `src/main.rs`
- Test: inline modules in the files above

- [ ] **Step 1: Add the test-only WAV helper**

In `src/main.rs`, after the `Notice` enum, add:

```rust
#[cfg(test)]
pub(crate) mod testutil {
    pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + samples.len() * 2);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&44_100u32.to_le_bytes());
        out.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }
}
```

- [ ] **Step 2: Rewrite `src/listener.rs`**

Replace the whole file with:

```rust
use rdev::{EventType, Key};

pub fn classify(held: &mut Vec<Key>, event: &EventType) -> Option<u16> {
    match event {
        EventType::KeyPress(key) => {
            if held.contains(key) {
                return None;
            }
            held.push(*key);
            key_to_scan_code(*key)
        }
        EventType::KeyRelease(key) => {
            held.retain(|k| k != key);
            None
        }
        _ => None,
    }
}

pub fn key_to_scan_code(key: Key) -> Option<u16> {
    use Key::*;
    Some(match key {
        Escape => 1,
        Num1 => 2,
        Num2 => 3,
        Num3 => 4,
        Num4 => 5,
        Num5 => 6,
        Num6 => 7,
        Num7 => 8,
        Num8 => 9,
        Num9 => 10,
        Num0 => 11,
        Minus => 12,
        Equal => 13,
        Backspace => 14,
        Tab => 15,
        KeyQ => 16,
        KeyW => 17,
        KeyE => 18,
        KeyR => 19,
        KeyT => 20,
        KeyY => 21,
        KeyU => 22,
        KeyI => 23,
        KeyO => 24,
        KeyP => 25,
        LeftBracket => 26,
        RightBracket => 27,
        Return => 28,
        ControlLeft => 29,
        KeyA => 30,
        KeyS => 31,
        KeyD => 32,
        KeyF => 33,
        KeyG => 34,
        KeyH => 35,
        KeyJ => 36,
        KeyK => 37,
        KeyL => 38,
        SemiColon => 39,
        Quote => 40,
        BackQuote => 41,
        ShiftLeft => 42,
        BackSlash | IntlBackslash => 43,
        KeyZ => 44,
        KeyX => 45,
        KeyC => 46,
        KeyV => 47,
        KeyB => 48,
        KeyN => 49,
        KeyM => 50,
        Comma => 51,
        Dot => 52,
        Slash => 53,
        ShiftRight => 54,
        KpMultiply => 55,
        Alt => 56,
        Space => 57,
        CapsLock => 58,
        F1 => 59,
        F2 => 60,
        F3 => 61,
        F4 => 62,
        F5 => 63,
        F6 => 64,
        F7 => 65,
        F8 => 66,
        F9 => 67,
        F10 => 68,
        NumLock => 69,
        ScrollLock => 70,
        Kp7 => 71,
        Kp8 => 72,
        Kp9 => 73,
        KpMinus => 74,
        Kp4 => 75,
        Kp5 => 76,
        Kp6 => 77,
        KpPlus => 78,
        Kp1 => 79,
        Kp2 => 80,
        Kp3 => 81,
        Kp0 => 82,
        KpDelete => 83,
        F11 => 87,
        F12 => 88,
        KpReturn => 3612,
        ControlRight => 3613,
        KpDivide => 3637,
        PrintScreen => 3639,
        AltGr => 3640,
        Pause => 3653,
        Home => 3655,
        PageUp => 3657,
        End => 3663,
        PageDown => 3665,
        Insert => 3666,
        Delete => 3667,
        MetaLeft => 3675,
        MetaRight => 3676,
        UpArrow => 57416,
        LeftArrow => 57419,
        RightArrow => 57421,
        DownArrow => 57424,
        Function | Unknown(_) => return None,
    })
}

#[cfg(target_os = "macos")]
const HINT: &str =
    "grant Input Monitoring (and Accessibility) to your terminal in System Settings → Privacy & Security, then restart brokey";

#[cfg(target_os = "linux")]
const HINT: &str = "needs an X11 session; on Wayland only XWayland apps are heard";

#[cfg(target_os = "windows")]
const HINT: &str = "could not install keyboard hook";

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const HINT: &str = "check OS permissions for global keyboard input";

pub fn listen(mut on_sound: impl FnMut(u16) + 'static) -> Result<(), String> {
    let mut held: Vec<Key> = Vec::new();
    rdev::listen(move |event| {
        if let Some(code) = classify(&mut held, &event.event_type) {
            on_sound(code);
        }
    })
    .map_err(|err| format!("{err:?}: {HINT}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdev::{EventType::*, Key};

    #[test]
    fn classify_press_and_space() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some(30));
        assert_eq!(classify(&mut held, &KeyPress(Key::Space)), Some(57));
    }

    #[test]
    fn classify_ignores_auto_repeat() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some(30));
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), None);
        assert_eq!(classify(&mut held, &KeyRelease(Key::KeyA)), None);
        assert_eq!(classify(&mut held, &KeyPress(Key::KeyA)), Some(30));
    }

    #[test]
    fn classify_ignores_non_key_events() {
        let mut held = vec![];
        assert_eq!(classify(&mut held, &MouseMove { x: 1.0, y: 1.0 }), None);
        assert_eq!(classify(&mut held, &ButtonPress(rdev::Button::Left)), None);
    }

    #[test]
    fn maps_representative_keys() {
        assert_eq!(key_to_scan_code(Key::KeyA), Some(30));
        assert_eq!(key_to_scan_code(Key::KeyZ), Some(44));
        assert_eq!(key_to_scan_code(Key::Num1), Some(2));
        assert_eq!(key_to_scan_code(Key::Num0), Some(11));
        assert_eq!(key_to_scan_code(Key::Minus), Some(12));
        assert_eq!(key_to_scan_code(Key::Equal), Some(13));
        assert_eq!(key_to_scan_code(Key::LeftBracket), Some(26));
        assert_eq!(key_to_scan_code(Key::SemiColon), Some(39));
        assert_eq!(key_to_scan_code(Key::Quote), Some(40));
        assert_eq!(key_to_scan_code(Key::BackQuote), Some(41));
        assert_eq!(key_to_scan_code(Key::BackSlash), Some(43));
        assert_eq!(key_to_scan_code(Key::IntlBackslash), Some(43));
        assert_eq!(key_to_scan_code(Key::Comma), Some(51));
        assert_eq!(key_to_scan_code(Key::Dot), Some(52));
        assert_eq!(key_to_scan_code(Key::Slash), Some(53));
        assert_eq!(key_to_scan_code(Key::Space), Some(57));
        assert_eq!(key_to_scan_code(Key::Return), Some(28));
        assert_eq!(key_to_scan_code(Key::Backspace), Some(14));
        assert_eq!(key_to_scan_code(Key::Tab), Some(15));
        assert_eq!(key_to_scan_code(Key::Escape), Some(1));
        assert_eq!(key_to_scan_code(Key::UpArrow), Some(57416));
        assert_eq!(key_to_scan_code(Key::LeftArrow), Some(57419));
        assert_eq!(key_to_scan_code(Key::RightArrow), Some(57421));
        assert_eq!(key_to_scan_code(Key::DownArrow), Some(57424));
        assert_eq!(key_to_scan_code(Key::Unknown(999)), None);
        assert_eq!(key_to_scan_code(Key::Function), None);
    }
}
```

- [ ] **Step 3: Rewrite `src/profiles.rs`**

Replace the whole file with:

```rust
use crate::pack::{LoadedPack, Manifest};
use std::fs;
use std::path::Path;

#[derive(Clone)]
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

#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub source: ProfileSource,
}

fn builtin(
    id: &str,
    name: &str,
    sprite: &'static [u8],
    manifest: &'static [u8],
) -> Result<Profile, String> {
    Manifest::parse(manifest)
        .map_err(|e| format!("Built-in profile '{id}' has an invalid manifest: {e}"))?;
    Ok(Profile {
        id: id.to_string(),
        name: name.to_string(),
        source: ProfileSource::Builtin { sprite, manifest },
    })
}

pub fn builtins() -> (Vec<Profile>, Vec<String>) {
    let specs: [(&str, &str, &'static [u8], &'static [u8]); 11] = [
        (
            "mx-blue",
            "Cherry MX Blue",
            include_bytes!("../assets/sounds/mx-blue/sound.flac"),
            include_bytes!("../assets/sounds/mx-blue/config.json"),
        ),
        (
            "mx-red",
            "Cherry MX Red",
            include_bytes!("../assets/sounds/mx-red/sound.flac"),
            include_bytes!("../assets/sounds/mx-red/config.json"),
        ),
        (
            "mx-brown",
            "Cherry MX Brown",
            include_bytes!("../assets/sounds/mx-brown/sound.flac"),
            include_bytes!("../assets/sounds/mx-brown/config.json"),
        ),
        (
            "topre",
            "Topre",
            include_bytes!("../assets/sounds/topre/sound.flac"),
            include_bytes!("../assets/sounds/topre/config.json"),
        ),
        (
            "mx-black",
            "Cherry MX Black",
            include_bytes!("../assets/sounds/mx-black/sound.flac"),
            include_bytes!("../assets/sounds/mx-black/config.json"),
        ),
        (
            "holy-panda",
            "Holy Panda",
            include_bytes!("../assets/sounds/holy-panda/sound.flac"),
            include_bytes!("../assets/sounds/holy-panda/config.json"),
        ),
        (
            "nk-cream",
            "NK Cream",
            include_bytes!("../assets/sounds/nk-cream/sound.flac"),
            include_bytes!("../assets/sounds/nk-cream/config.json"),
        ),
        (
            "eg-oreo",
            "Everglide Oreo",
            include_bytes!("../assets/sounds/eg-oreo/sound.flac"),
            include_bytes!("../assets/sounds/eg-oreo/config.json"),
        ),
        (
            "eg-crystal-purple",
            "Everglide Crystal Purple",
            include_bytes!("../assets/sounds/eg-crystal-purple/sound.flac"),
            include_bytes!("../assets/sounds/eg-crystal-purple/config.json"),
        ),
        (
            "turquoise",
            "Turquoise Tealios",
            include_bytes!("../assets/sounds/turquoise/sound.flac"),
            include_bytes!("../assets/sounds/turquoise/config.json"),
        ),
        (
            "mx-black-abs",
            "Cherry MX Black (ABS)",
            include_bytes!("../assets/sounds/mx-black-abs/sound.flac"),
            include_bytes!("../assets/sounds/mx-black-abs/config.json"),
        ),
    ];

    let mut profiles = Vec::with_capacity(specs.len());
    let mut warnings = Vec::new();
    for (id, name, sprite, manifest) in specs {
        match builtin(id, name, sprite, manifest) {
            Ok(profile) => profiles.push(profile),
            Err(e) => warnings.push(e),
        }
    }
    (profiles, warnings)
}

pub fn load_user(dir: &Path) -> (Vec<Profile>, Vec<String>) {
    if !dir.exists() {
        return (vec![], vec![]);
    }
    let mut profiles = Vec::new();
    let mut warnings = Vec::new();
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            warnings.push(format!("Skipped sound pack '?': {}", e));
            return (vec![], warnings);
        }
    };
    let mut entries: Vec<std::fs::DirEntry> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let id = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }

        let mut pack_entries: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(prd) = fs::read_dir(&p) {
            for pe in prd.flatten() {
                let pp = pe.path();
                if pp.is_file() {
                    pack_entries.push(pp);
                }
            }
        }

        let mut press_cands: Vec<std::path::PathBuf> = pack_entries
            .iter()
            .filter(|pp| pp.file_stem().and_then(|s| s.to_str()) == Some("press"))
            .cloned()
            .collect();
        press_cands.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        let press_path = match press_cands.first() {
            Some(pp) => pp.clone(),
            None => {
                warnings.push(format!("Skipped sound pack '{}': no press sound", id));
                continue;
            }
        };
        let press_bytes = match fs::read(&press_path) {
            Ok(b) => b,
            Err(e) => {
                warnings.push(format!("Skipped sound pack '{}': {}", id, e));
                continue;
            }
        };

        let mut space_cands: Vec<std::path::PathBuf> = pack_entries
            .iter()
            .filter(|pp| pp.file_stem().and_then(|s| s.to_str()) == Some("space"))
            .cloned()
            .collect();
        space_cands.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        let space_bytes = space_cands
            .first()
            .and_then(|sp| fs::read(sp).ok())
            .filter(|b| LoadedPack::from_custom(b.clone(), None).is_ok());

        if let Err(e) = LoadedPack::from_custom(press_bytes.clone(), space_bytes.clone()) {
            warnings.push(format!("Skipped sound pack '{}': {}", id, e));
            continue;
        }

        profiles.push(Profile {
            id: id.clone(),
            name: id.clone(),
            source: ProfileSource::Custom {
                press: press_bytes,
                space: space_bytes,
            },
        });
    }
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    (profiles, warnings)
}

pub fn all(user_dir: Option<&Path>) -> (Vec<Profile>, Vec<String>) {
    let (mut result, mut warnings) = builtins();
    if let Some(dir) = user_dir {
        let (users, mut uw) = load_user(dir);
        for u in users {
            if result.iter().any(|b| b.id == u.id) {
                warnings.push(format!(
                    "Skipped sound pack '{}': conflicts with a built-in profile",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::wav_bytes;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn builtins_are_the_expected_switches() {
        let (b, warnings) = builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        let ids: Vec<&str> = b.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "mx-blue",
                "mx-red",
                "mx-brown",
                "topre",
                "mx-black",
                "holy-panda",
                "nk-cream",
                "eg-oreo",
                "eg-crystal-purple",
                "turquoise",
                "mx-black-abs",
            ]
        );
        let names: Vec<&str> = b.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Cherry MX Blue",
                "Cherry MX Red",
                "Cherry MX Brown",
                "Topre",
                "Cherry MX Black",
                "Holy Panda",
                "NK Cream",
                "Everglide Oreo",
                "Everglide Crystal Purple",
                "Turquoise Tealios",
                "Cherry MX Black (ABS)",
            ]
        );
    }

    #[test]
    fn all_builtins_decode() {
        let (b, warnings) = builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        for profile in &b {
            match &profile.source {
                ProfileSource::Builtin { sprite, manifest } => {
                    let pack = LoadedPack::from_builtin(*sprite, *manifest)
                        .unwrap_or_else(|e| panic!("{}: {e}", profile.id));
                    assert!(!pack.keys.is_empty(), "{} has no keys", profile.id);
                    assert!(!pack.defaults.is_empty(), "{} has no defaults", profile.id);
                }
                ProfileSource::Custom { .. } => panic!("{} is not built-in", profile.id),
            }
        }
    }

    #[test]
    fn mx_blue_has_per_key_letters_and_space() {
        let (b, _) = builtins();
        let profile = b.iter().find(|p| p.id == "mx-blue").unwrap();
        match &profile.source {
            ProfileSource::Builtin { manifest, .. } => {
                let m = Manifest::parse(*manifest).unwrap();
                assert!(m.keys.contains_key(&30)); // A
                assert!(m.keys.contains_key(&57)); // Space
            }
            ProfileSource::Custom { .. } => panic!("mx-blue is not built-in"),
        }
    }

    #[test]
    fn user_pack_loads() {
        let td = tempdir().unwrap();
        let pack = td.path().join("mine");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
        let (ps, warns) = load_user(td.path());
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].id, "mine");
        assert!(matches!(
            &ps[0].source,
            ProfileSource::Custom { space: None, .. }
        ));
        assert!(warns.is_empty());
    }

    #[test]
    fn user_pack_maps_space() {
        let td = tempdir().unwrap();
        let pack = td.path().join("mine");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
        fs::write(pack.join("space.wav"), wav_bytes(&[0, 500, -500, 0])).unwrap();
        let (ps, warns) = load_user(td.path());
        assert_eq!(ps.len(), 1);
        assert!(matches!(
            &ps[0].source,
            ProfileSource::Custom { space: Some(_), .. }
        ));
        assert!(warns.is_empty());
    }

    #[test]
    fn user_pack_without_press_is_skipped() {
        let td = tempdir().unwrap();
        let pack = td.path().join("empty");
        fs::create_dir_all(&pack).unwrap();
        let (ps, warns) = load_user(td.path());
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("empty"));
    }

    #[test]
    fn user_pack_with_garbage_is_skipped() {
        let td = tempdir().unwrap();
        let pack = td.path().join("bad");
        fs::create_dir_all(&pack).unwrap();
        let mut f = File::create(pack.join("press.wav")).unwrap();
        f.write_all(b"nope").unwrap();
        let (ps, warns) = load_user(td.path());
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("bad"));
    }

    #[test]
    fn user_pack_cannot_shadow_builtin() {
        let td = tempdir().unwrap();
        let pack = td.path().join("mx-blue");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
        let (allp, warns) = all(Some(td.path()));
        assert_eq!(allp.len(), builtins().0.len());
        assert!(warns.iter().any(|w| w.contains("conflicts")));
    }

    #[test]
    fn missing_user_dir_is_fine() {
        let (ps, warns) = load_user(Path::new("/nonexistent/brokey"));
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 0);
    }
}
```

- [ ] **Step 4: Rewrite `src/audio.rs`**

Replace the whole file with:

```rust
use crate::pack::{LoadedPack, PlaybackClip};
use crate::profiles::{Profile, ProfileSource};
use crate::Notice;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStreamBuilder, Source};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(u16),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
}

fn load(profile: &Profile) -> Result<LoadedPack, String> {
    match &profile.source {
        ProfileSource::Builtin { sprite, manifest } => {
            LoadedPack::from_builtin(*sprite, *manifest)
        }
        ProfileSource::Custom { press, space } => {
            LoadedPack::from_custom(press.clone(), space.clone())
        }
    }
}

pub fn spawn(
    profiles: Vec<Profile>,
    selected: usize,
    volume: u8,
    notices: Sender<Notice>,
) -> Sender<AudioCmd> {
    let (tx, rx) = std::sync::mpsc::channel::<AudioCmd>();
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
        let mut volume_val = volume;
        let mut paused = false;
        let mut pack = profiles.get(selected).and_then(|profile| match load(profile) {
            Ok(pack) => Some(pack),
            Err(e) => {
                let _ = notices.send(Notice::Warn(format!(
                    "Could not load '{}': {e}",
                    profile.name
                )));
                None
            }
        });

        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(scan_code) => {
                    if paused {
                        continue;
                    }
                    if let Some(pack) = pack.as_mut() {
                        let clip = pack.clip_for(scan_code);
                        play(&stream, &clip, volume_val);
                    }
                }
                AudioCmd::Preview => {
                    if let Some(pack) = pack.as_mut() {
                        let clip = pack.next_default();
                        play(&stream, &clip, volume_val);
                    }
                }
                AudioCmd::SetProfile(i) => {
                    let profile = match profiles.get(i) {
                        Some(p) => p,
                        None => continue,
                    };
                    match load(profile) {
                        Ok(new_pack) => {
                            pack = Some(new_pack);
                        }
                        Err(e) => {
                            let _ = notices.send(Notice::Warn(format!(
                                "Could not load '{}': {e}",
                                profile.name
                            )));
                        }
                    }
                }
                AudioCmd::SetVolume(v) => {
                    volume_val = v;
                }
                AudioCmd::SetPaused(p) => {
                    paused = p;
                }
            }
        }
    });
    tx
}

fn play(stream: &rodio::OutputStream, clip: &PlaybackClip, volume: u8) {
    stream.mixer().add(
        SamplesBuffer::new(clip.channels, clip.rate, clip.data())
            .amplify(volume as f32 / 100.0),
    );
}

fn drain_commands(rx: Receiver<AudioCmd>) {
    while rx.recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profile_loads() {
        let (profiles, warnings) = crate::profiles::builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!profiles.is_empty());
        assert!(load(&profiles[0]).is_ok());
    }

    #[test]
    fn custom_profile_loads() {
        let wav = crate::testutil::wav_bytes(&[0, 1000, -1000, 0]);
        let profile = Profile {
            id: "mine".to_string(),
            name: "mine".to_string(),
            source: ProfileSource::Custom { press: wav, space: None },
        };
        assert!(load(&profile).is_ok());
    }
}
```

- [ ] **Step 5: Run the full test suite**

Run:

```bash
cargo test
```

Expected: all tests pass, including `pack::tests`, `profiles::tests`,
`listener::tests`, and `audio::tests`.

- [ ] **Step 6: Run clippy**

Run:

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add src/listener.rs src/profiles.rs src/audio.rs src/main.rs
git commit -m "feat: play per-key clips from lazily loaded packs"
```

---

## Chunk 4: UI coverage, cleanup, and docs

### Task 5: Cover the 11-profile list in the UI

**Files:**
- Modify: `src/ui.rs` (test module only)

- [ ] **Step 1: Add the UI test**

Append this test inside the existing `#[cfg(test)] mod tests` in `src/ui.rs`:

```rust
    #[test]
    fn renders_all_builtin_profiles() {
        let (builtins, warnings) = crate::profiles::builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        let pairs: Vec<(String, String)> = builtins
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        let a = new(pairs, &Config::default());
        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Cherry MX Black (ABS)"));
        assert!(text.contains("Turquoise Tealios"));
    }
```

- [ ] **Step 2: Run the UI tests**

Run:

```bash
cargo test ui::tests
```

Expected: all UI tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "test: render all eleven built-in profiles"
```

---

### Task 6: Remove obsolete assets and update documentation

**Files:**
- Delete: `assets/sounds/*/press.wav`
- Delete: `assets/sounds/*/space.wav`
- Modify: `README.md`
- Modify: `assets/sounds/CREDITS.md`

- [ ] **Step 1: Delete the old single-clip assets**

Run:

```bash
git rm assets/sounds/*/press.wav assets/sounds/*/space.wav
```

Verify nothing in the compiled code still embeds them:

```bash
grep -R "assets/sounds/.*press.wav\|assets/sounds/.*space.wav" src || true
```

Expected: no output.

- [ ] **Step 2: Update the README built-in list and privacy wording**

In `README.md`, append the new profile to the built-in list:

```markdown
- `mx-black-abs` "Cherry MX Black (ABS)"
```

Immediately after the list, add:

```markdown
Built-in profiles use real per-key recordings: letters, digits, common
punctuation, Space, Enter, Backspace, and Tab each have their own sample where
the source pack provides one. Keys without a recording, and the preview key,
rotate through natural fallback takes from the same pack.
```

Replace the final privacy sentence with:

```markdown
Brokey uses the keycode only to choose a sound; it does not build, log, or store
text. No audio is recorded.
```

- [ ] **Step 3: Rewrite `assets/sounds/CREDITS.md`**

Replace the whole file with:

```markdown
# Sound Credits

Every file in this directory is a real switch recording, redistributed under the MIT
license. No placeholder noise is left.

## Format

Each built-in profile contains:

- `sound.flac` — mono, 44.1 kHz, 16-bit FLAC. It holds only the typing-key clips
  (letters, digits, common punctuation, Space, Enter, Backspace, Tab) concatenated in
  ascending MechVibes scan-code order.
- `config.json` — manifest with sample offsets into `sound.flac`.

Clips are cut from each pack's own sprite map or per-key files, given a 2 ms fade-in
and 6 ms fade-out (each clamped to half the clip length), and the whole pack is
normalized to a −2 dBFS peak. Normalizing the whole pack preserves the relative
loudness between keys. The files are generated by `scripts/build_sound_packs.py`;
normal builds do not need ffmpeg or network access.

## Sources

| Directory           | Switch                    | Source pack                                                                                              | Author                    | License |
| ------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------- | ------- |
| `mx-blue`           | Cherry MX Blue (ABS)      | [`cherrymx-blue-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-blue-abs)             | Connor Langan / MechVibes | MIT     |
| `mx-red`            | Cherry MX Red (ABS)       | [`cherrymx-red-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-red-abs)               | Connor Langan / MechVibes | MIT     |
| `mx-brown`          | Cherry MX Brown (ABS)     | [`cherrymx-brown-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-brown-abs)           | Connor Langan / MechVibes | MIT     |
| `topre`             | Topre Purple Hybrid (PBT) | [`topre-purple-hybrid-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/topre-purple-hybrid-pbt) | Connor Langan / MechVibes | MIT     |
| `mx-black`          | Cherry MX Black (PBT)     | [`cherrymx-black-pbt`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-black-pbt)           | Connor Langan / MechVibes | MIT     |
| `mx-black-abs`      | Cherry MX Black (ABS)     | [`cherrymx-black-abs`](https://github.com/cjlangan/MechSim/tree/main/audio/cherrymx-black-abs)           | Connor Langan / MechVibes | MIT     |
| `holy-panda`        | Holy Panda                | [`holy-pandas`](https://github.com/cjlangan/MechSim/tree/main/audio/holy-pandas)                         | Connor Langan / MechVibes | MIT     |
| `nk-cream`          | NK Cream                  | [`nk-cream`](https://github.com/cjlangan/MechSim/tree/main/audio/nk-cream)                               | Connor Langan / MechVibes | MIT     |
| `eg-oreo`           | Everglide Oreo            | [`eg-oreo`](https://github.com/cjlangan/MechSim/tree/main/audio/eg-oreo)                                 | Connor Langan / MechVibes | MIT     |
| `eg-crystal-purple` | Everglide Crystal Purple  | [`eg-crystal-purple`](https://github.com/cjlangan/MechSim/tree/main/audio/eg-crystal-purple)             | Connor Langan / MechVibes | MIT     |
| `turquoise`         | Turquoise Tealios         | [`turquoise`](https://github.com/cjlangan/MechSim/tree/main/audio/turquoise)                             | Connor Langan / MechVibes | MIT     |

`mx-black-abs` is the same Cherry MX Black ABS recording that MechVibes distributes;
MechSim's copy is byte-identical. MechSim took its audio and frame maps from
[MechVibes](https://github.com/hainguyents13/mechvibes).

## License notices

MIT License, Copyright (c) 2025 Connor Langan (MechSim), Copyright (c) 2021 Hai Nguyen
(MechVibes).

Permission is hereby granted, free of charge, to any person obtaining a copy of this
software and associated documentation files (the "Software"), to deal in the Software
without restriction, including without limitation the rights to use, copy, modify,
merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice shall be included in all copies
or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF
CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

- [ ] **Step 4: Verify the docs mention the new profile and privacy wording**

Run:

```bash
grep -n "mx-black-abs" README.md
grep -n "uses the keycode only" README.md
grep -n "cherrymx-black-abs" assets/sounds/CREDITS.md
```

Expected: each command prints at least one matching line.

- [ ] **Step 5: Commit**

```bash
git add README.md assets/sounds/CREDITS.md
git commit -m "docs: document per-key sounds and credits"
```

Note: `git rm` already staged the deleted WAVs; they are included in this commit.

---

### Task 7: Final verification

**Files:** none

- [ ] **Step 1: Run formatting**

```bash
cargo fmt --all
```

Expected: no output.

- [ ] **Step 2: Run the full test suite**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 3: Run clippy with warnings denied**

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: no warnings.

- [ ] **Step 4: Build release**

```bash
cargo build --release
```

Expected: success.

- [ ] **Step 5: Check committed audio size**

```bash
find assets/sounds -name sound.flac -exec du -ch {} + | tail -1
```

Expected: under 4.0M.

- [ ] **Step 6: Confirm no old assets remain**

```bash
find assets/sounds -name 'press.*' -o -name 'space.*'
```

Expected: no output.

- [ ] **Step 7: Commit formatting changes if any**

```bash
git status --short
```

If `cargo fmt` changed files:

```bash
git add -u
git commit -m "style: format per-key sound implementation"
```

- [ ] **Step 8: Manual check (user)**

Run `cargo run --release` and verify:

- The list shows 11 profiles including "Cherry MX Black (ABS)".
- Typing different letters on a pack with per-key data produces different sounds.
- Space, Enter, Backspace, and Tab use their own samples.
- Switching profiles previews the new pack.
- Pause, volume, and quit still work.
- Holy Panda and Turquoise still click (fallback takes) even though they have no
  per-key letters.

---

## Acceptance criteria

- 11 built-in profiles are listed; `mx-black-abs` plays the requested Cherry MX
  Black ABS recording.
- Typing letters in packs that provide per-key data produces key-specific samples;
  packs without it rotate through fallback takes.
- Space uses the space slice when the pack has one.
- Keys without a specific slice still produce a click via defaults.
- Total committed audio is under 4 MB.
- Existing custom packs load and play unchanged.
- `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- No typed characters or text are logged or stored.
