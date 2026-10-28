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
                    let pack = LoadedPack::from_builtin(sprite, manifest)
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
                let m = Manifest::parse(manifest).unwrap();
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
