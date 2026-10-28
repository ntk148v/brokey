use crate::pack::{LoadedPack, Manifest};
use std::fs;
use std::path::Path;

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
        release: Option<Vec<u8>>,
    },
    /// A user folder in the bundled format: config.json + sound.flac.
    Sprite { sprite: Vec<u8>, manifest: Vec<u8> },
}

#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub source: ProfileSource,
}

impl Profile {
    pub fn is_optional(&self) -> bool {
        matches!(&self.source, ProfileSource::Optional)
    }

    pub fn is_available(&self, packs: Option<&Path>) -> bool {
        match &self.source {
            ProfileSource::Optional => packs
                .map(|dir| crate::download::is_installed(dir, &self.id))
                .unwrap_or(false),
            ProfileSource::Builtin { .. }
            | ProfileSource::Custom { .. }
            | ProfileSource::Sprite { .. } => true,
        }
    }
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

        if let (Ok(manifest), Ok(sprite)) =
            (fs::read(p.join("config.json")), fs::read(p.join("sound.flac")))
        {
            match LoadedPack::from_builtin(&sprite, &manifest) {
                Ok(_) => profiles.push(Profile {
                    id: id.clone(),
                    name: id.clone(),
                    source: ProfileSource::Sprite { sprite, manifest },
                }),
                Err(e) => warnings.push(format!("Skipped sound pack '{}': {}", id, e)),
            }
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

        let optional_clip = |stem: &str| {
            let mut cands: Vec<&std::path::PathBuf> = pack_entries
                .iter()
                .filter(|pp| pp.file_stem().and_then(|s| s.to_str()) == Some(stem))
                .collect();
            cands.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
            cands
                .first()
                .and_then(|sp| fs::read(sp).ok())
                .filter(|b| LoadedPack::from_custom(b.clone(), None, None).is_ok())
        };
        let space_bytes = optional_clip("space");
        let release_bytes = optional_clip("release");

        if let Err(e) = LoadedPack::from_custom(press_bytes.clone(), space_bytes.clone(), None) {
            warnings.push(format!("Skipped sound pack '{}': {}", id, e));
            continue;
        }

        profiles.push(Profile {
            id: id.clone(),
            name: id.clone(),
            source: ProfileSource::Custom {
                press: press_bytes,
                space: space_bytes,
                release: release_bytes,
            },
        });
    }
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    (profiles, warnings)
}

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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::wav_bytes;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    pub(crate) fn embedded_bytes() -> (&'static [u8], &'static [u8]) {
        let (profiles, _) = builtins();
        match &profiles[0].source {
            ProfileSource::Builtin { sprite, manifest } => (*sprite, *manifest),
            _ => panic!("first builtin is not embedded"),
        }
    }

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
                ProfileSource::Optional
                | ProfileSource::Custom { .. }
                | ProfileSource::Sprite { .. } => {
                    panic!("{} is not embedded", profile.id)
                }
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
            _ => panic!("mx-blue is not built-in"),
        }
    }

    #[test]
    fn availability_tracks_the_cache() {
        let td = tempdir().unwrap();
        let (sprite, manifest) = embedded_bytes();
        crate::download::install_from_bytes(td.path(), "mx-blue-pbt", manifest, sprite).unwrap();
        let b = builtins().0;
        assert!(b[0].is_available(None));
        let o = optional();
        let p = o.iter().find(|p| p.id == "mx-blue-pbt").unwrap();
        assert!(p.is_available(Some(td.path())));
        assert!(!p.is_available(None));
        assert!(!o
            .iter()
            .find(|p| p.id == "holy-panda")
            .unwrap()
            .is_available(Some(td.path())));
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
    fn user_pack_maps_release() {
        let td = tempdir().unwrap();
        let pack = td.path().join("mine");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("press.wav"), wav_bytes(&[0, 1000, -1000, 0])).unwrap();
        fs::write(pack.join("release.wav"), wav_bytes(&[0, 500, -500, 0])).unwrap();
        let (ps, warns) = load_user(td.path());
        assert!(warns.is_empty(), "{warns:?}");
        let ProfileSource::Custom { press, space, release } = &ps[0].source else {
            panic!("not a custom pack");
        };
        let mut loaded =
            LoadedPack::from_custom(press.clone(), space.clone(), release.clone()).unwrap();
        assert!(loaded.next_release().is_some());
    }

    #[test]
    fn user_sprite_pack_loads() {
        let (sprite, manifest) = embedded_bytes();
        let td = tempdir().unwrap();
        let pack = td.path().join("my-blue");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("config.json"), manifest).unwrap();
        fs::write(pack.join("sound.flac"), sprite).unwrap();
        let (ps, warns) = load_user(td.path());
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(ps[0].id, "my-blue");
        assert!(matches!(&ps[0].source, ProfileSource::Sprite { .. }));
    }

    #[test]
    fn user_sprite_pack_with_bad_manifest_is_skipped() {
        let (sprite, _) = embedded_bytes();
        let td = tempdir().unwrap();
        let pack = td.path().join("broken");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("config.json"), b"{}").unwrap();
        fs::write(pack.join("sound.flac"), sprite).unwrap();
        let (ps, warns) = load_user(td.path());
        assert!(ps.is_empty());
        assert!(warns[0].contains("broken"));
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
        assert_eq!(allp.len(), 18);
        assert!(warns.iter().any(|w| w.contains("conflicts")));
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

    #[test]
    fn missing_user_dir_is_fine() {
        let (ps, warns) = load_user(Path::new("/nonexistent/brokey"));
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 0);
    }

    #[test]
    fn optional_assets_decode_from_disk() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/sounds-optional");
        let mut ids: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        ids.sort();
        assert_eq!(ids.len(), 14, "{ids:?}");
        for id in ids {
            let dir = root.join(&id);
            let manifest = fs::read(dir.join("config.json")).unwrap();
            let sprite = fs::read(dir.join("sound.flac")).unwrap();
            let pack = LoadedPack::from_builtin(&sprite, &manifest)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!pack.defaults.is_empty(), "{id} has no defaults");
            // Upstream mxblue-travel currently ships only generic takes.
            if id != "mx-blue-travel" {
                assert!(!pack.keys.is_empty(), "{id} has no keys");
            }
        }
    }
}
