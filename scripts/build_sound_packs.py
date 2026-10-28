#!/usr/bin/env python3
"""Build brokey's trimmed per-key sound packs from MechSim recordings.

This is a maintenance tool. The generated `sound.flac` and `config.json`
files are committed, so normal builds and runtime need neither network access
nor ffmpeg.

Usage:
    python3 scripts/build_sound_packs.py [--set {default,optional}] [--only ID ...]
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


def build(profile: str, source_dir: str, out_root: Path) -> tuple[int, int]:
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
                try:
                    download(url_for(source_dir, rel), path)
                except RuntimeError as exc:
                    print(
                        f"warning: {profile}: skipping key {code}: {exc}",
                        file=sys.stderr,
                    )
                    continue
                clip = ffmpeg_pcm(path)
            clip = apply_fades(clip)
            if not clip:
                continue
            keys[code] = (len(buf), len(clip))
            buf.extend(clip)

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
        if not buf:
            raise RuntimeError(f"{profile}: no clips found")

        peak = max((abs(sample) for sample in buf), default=0.0)
        if peak > 0.0:
            gain = PEAK / peak
            buf = [sample * gain for sample in buf]

        dest_dir = out_root / profile
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


if __name__ == "__main__":
    raise SystemExit(main())
