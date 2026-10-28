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
    pub release: Vec<(usize, usize)>,
}

#[derive(Deserialize)]
struct RawManifest {
    version: u32,
    sample_rate: u32,
    channels: u16,
    total_samples: usize,
    keys: HashMap<String, [usize; 2]>,
    defaults: Vec<[usize; 2]>,
    #[serde(default)]
    release: Vec<[usize; 2]>,
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

        let mut release = Vec::with_capacity(raw.release.len());
        for [start, len] in raw.release {
            check_range(start, len, raw.total_samples)?;
            release.push((start, len));
        }

        Ok(Manifest {
            sample_rate: raw.sample_rate,
            channels: raw.channels,
            total_samples: raw.total_samples,
            keys,
            defaults,
            release,
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
    pub release: Vec<PlaybackClip>,
    next_default: usize,
    next_release: usize,
}

impl LoadedPack {
    pub fn from_builtin(sprite: &[u8], manifest: &[u8]) -> Result<LoadedPack, String> {
        let manifest = Manifest::parse(manifest)?;
        let decoder = Decoder::new(Cursor::new(sprite.to_vec()))
            .map_err(|e| format!("could not decode FLAC: {e}"))?;
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
        let clips = |ranges: Vec<(usize, usize)>| -> Vec<PlaybackClip> {
            ranges
                .into_iter()
                .map(|(start, len)| PlaybackClip {
                    samples: Arc::clone(&samples),
                    channels,
                    rate,
                    start,
                    len,
                })
                .collect()
        };
        let defaults = clips(manifest.defaults);
        let release = clips(manifest.release);

        Ok(LoadedPack {
            keys,
            defaults,
            release,
            next_default: 0,
            next_release: 0,
        })
    }

    pub fn from_custom(
        press: Vec<u8>,
        space: Option<Vec<u8>>,
        release: Option<Vec<u8>>,
    ) -> Result<LoadedPack, String> {
        let press = decode_clip(press).map_err(|e| format!("press: {e}"))?;
        let mut keys = HashMap::new();
        if let Some(bytes) = space {
            if let Ok(clip) = decode_clip(bytes) {
                keys.insert(57, clip);
            }
        }
        let release = release.and_then(|bytes| decode_clip(bytes).ok());
        Ok(LoadedPack {
            keys,
            defaults: vec![press],
            release: release.into_iter().collect(),
            next_default: 0,
            next_release: 0,
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

    /// None when the pack has no release sounds.
    pub fn next_release(&mut self) -> Option<PlaybackClip> {
        let clip = self.release.get(self.next_release)?.clone();
        self.next_release = (self.next_release + 1) % self.release.len();
        Some(clip)
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
        assert!(m.release.is_empty());
    }

    #[test]
    fn parses_and_checks_release() {
        let m = VALID.replace(
            "\"defaults\": [[0, 5]]",
            "\"defaults\": [[0, 5]], \"release\": [[5, 5]]",
        );
        assert_eq!(parse(&m).unwrap().release, vec![(5, 5)]);
        let bad = VALID.replace(
            "\"defaults\": [[0, 5]]",
            "\"defaults\": [[0, 5]], \"release\": [[8, 5]]",
        );
        assert!(parse(&bad).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn release_rotates_or_is_absent() {
        let mut pack = LoadedPack {
            keys: HashMap::new(),
            defaults: vec![clip(0)],
            release: vec![],
            next_default: 0,
            next_release: 0,
        };
        assert!(pack.next_release().is_none());
        pack.release = vec![clip(0), clip(4)];
        assert_eq!(pack.next_release().unwrap().start, 0);
        assert_eq!(pack.next_release().unwrap().start, 4);
        assert_eq!(pack.next_release().unwrap().start, 0);
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
            release: vec![],
            next_default: 0,
            next_release: 0,
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
            release: vec![],
            next_default: 0,
            next_release: 0,
        };
        pack.keys.insert(16, clip(4));
        assert_eq!(pack.clip_for(16).start, 4);
        assert_eq!(pack.clip_for(17).start, 0);
    }
}
