use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub profile: String,
    pub volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profile: "mx-blue".to_string(),
            volume: 60,
        }
    }
}

pub fn dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("brokey"))
}

pub fn load(path: &Path) -> (Config, Option<String>) {
    match fs::read_to_string(path) {
        Ok(s) => match toml::from_str::<Config>(&s) {
            Ok(mut cfg) => {
                if cfg.volume > 100 {
                    cfg.volume = 100;
                }
                (cfg, None)
            }
            Err(e) => {
                let w = format!("Ignored unreadable config {}: {}", path.display(), e);
                (Config::default(), Some(w))
            }
        },
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                return (Config::default(), None);
            }
            let w = format!("Ignored unreadable config {}: {}", path.display(), e);
            (Config::default(), Some(w))
        }
    }
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let toml_str = toml::to_string(cfg).map_err(|e| e.to_string())?;
    let tmp_path = if let Some(parent) = path.parent() {
        parent.join("config.toml.tmp")
    } else {
        PathBuf::from("config.toml.tmp")
    };
    fs::write(&tmp_path, toml_str).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_is_default() {
        let td = tempdir().unwrap();
        let p = td.path().join("missing.toml");
        let (cfg, w) = load(&p);
        assert_eq!(cfg, Config::default());
        assert!(w.is_none());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let td = tempdir().unwrap();
        let p = td.path().join("config.toml");
        let cfg = Config { profile: "topre".into(), volume: 30 };
        save(&p, &cfg).unwrap();
        let (loaded, w) = load(&p);
        assert_eq!(loaded, cfg);
        assert!(w.is_none());
    }

    #[test]
    fn load_corrupt_file_falls_back() {
        let td = tempdir().unwrap();
        let p = td.path().join("config.toml");
        fs::write(&p, "volume = [").unwrap();
        let (cfg, w) = load(&p);
        assert_eq!(cfg, Config::default());
        let w = w.expect("warning");
        assert!(w.contains("config"));
    }

    #[test]
    fn load_clamps_volume() {
        let td = tempdir().unwrap();
        let p = td.path().join("config.toml");
        fs::write(&p, "volume = 250").unwrap();
        let (cfg, w) = load(&p);
        assert_eq!(cfg.volume, 100);
        assert_eq!(cfg.profile, Config::default().profile);
        assert!(w.is_none());
    }

    #[test]
    fn save_creates_parent_dir() {
        let td = tempdir().unwrap();
        let p = td.path().join("a/b/config.toml");
        let cfg = Config::default();
        save(&p, &cfg).unwrap();
        assert!(p.exists());
    }
}
