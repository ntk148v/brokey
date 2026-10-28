use crate::audio::decode;
use crate::audio::Clip;
use std::fs;
use std::path::Path;

#[derive(Clone)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub press: Clip,
    pub space: Option<Clip>,
}

fn builtin(id: &str, name: &str, press: &[u8], space: &[u8]) -> Profile {
    Profile {
        id: id.to_string(),
        name: name.to_string(),
        press: decode(press.to_vec()).expect("built-in press sound must decode"),
        space: Some(decode(space.to_vec()).expect("built-in space sound must decode")),
    }
}

pub fn builtins() -> Vec<Profile> {
    vec![
        builtin(
            "mx-blue",
            "Cherry MX Blue",
            include_bytes!("../assets/sounds/mx-blue/press.wav"),
            include_bytes!("../assets/sounds/mx-blue/space.wav"),
        ),
        builtin(
            "mx-red",
            "Cherry MX Red",
            include_bytes!("../assets/sounds/mx-red/press.wav"),
            include_bytes!("../assets/sounds/mx-red/space.wav"),
        ),
        builtin(
            "mx-brown",
            "Cherry MX Brown",
            include_bytes!("../assets/sounds/mx-brown/press.wav"),
            include_bytes!("../assets/sounds/mx-brown/space.wav"),
        ),
        builtin(
            "topre",
            "Topre",
            include_bytes!("../assets/sounds/topre/press.wav"),
            include_bytes!("../assets/sounds/topre/space.wav"),
        ),
        builtin(
            "mx-black",
            "Cherry MX Black",
            include_bytes!("../assets/sounds/mx-black/press.wav"),
            include_bytes!("../assets/sounds/mx-black/space.wav"),
        ),
        builtin(
            "holy-panda",
            "Holy Panda",
            include_bytes!("../assets/sounds/holy-panda/press.wav"),
            include_bytes!("../assets/sounds/holy-panda/space.wav"),
        ),
        builtin(
            "nk-cream",
            "NK Cream",
            include_bytes!("../assets/sounds/nk-cream/press.wav"),
            include_bytes!("../assets/sounds/nk-cream/space.wav"),
        ),
        builtin(
            "eg-oreo",
            "Everglide Oreo",
            include_bytes!("../assets/sounds/eg-oreo/press.wav"),
            include_bytes!("../assets/sounds/eg-oreo/space.wav"),
        ),
        builtin(
            "eg-crystal-purple",
            "Everglide Crystal Purple",
            include_bytes!("../assets/sounds/eg-crystal-purple/press.wav"),
            include_bytes!("../assets/sounds/eg-crystal-purple/space.wav"),
        ),
        builtin(
            "turquoise",
            "Turquoise Tealios",
            include_bytes!("../assets/sounds/turquoise/press.wav"),
            include_bytes!("../assets/sounds/turquoise/space.wav"),
        ),
    ]
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
        // try to load pack
        // find press
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
        let press = match decode(press_bytes) {
            Ok(c) => c,
            Err(e) => {
                warnings.push(format!("Skipped sound pack '{}': {}", id, e));
                continue;
            }
        };
        // space optional
        let mut space_cands: Vec<std::path::PathBuf> = pack_entries
            .iter()
            .filter(|pp| pp.file_stem().and_then(|s| s.to_str()) == Some("space"))
            .cloned()
            .collect();
        space_cands.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
        let mut space_clip: Option<Clip> = None;
        if let Some(sp) = space_cands.first() {
            if let Ok(sb) = fs::read(sp) {
                if let Ok(sc) = decode(sb) {
                    space_clip = Some(sc);
                }
                // else skip space, no warning
            }
            // else skip space, no warning
        }
        profiles.push(Profile {
            id: id.clone(),
            name: id.clone(),
            press,
            space: space_clip,
        });
    }
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    (profiles, warnings)
}

pub fn all(user_dir: Option<&Path>) -> (Vec<Profile>, Vec<String>) {
    let mut result = builtins();
    let mut warnings = Vec::new();
    if let Some(dir) = user_dir {
        let (users, mut uw) = load_user(dir);
        for u in users {
            // check shadow
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
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn builtins_are_the_expected_switches() {
        let b = builtins();
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
                "turquoise"
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
                "Turquoise Tealios"
            ]
        );
        assert!(b.iter().all(|p| p.space.is_some()));
    }

    #[test]
    fn user_pack_loads() {
        let td = tempdir().unwrap();
        let pack = td.path().join("mine");
        fs::create_dir_all(&pack).unwrap();
        let press = include_bytes!("../assets/sounds/mx-blue/press.wav");
        let mut f = File::create(pack.join("press.wav")).unwrap();
        f.write_all(press).unwrap();
        let (ps, warns) = load_user(td.path());
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].id, "mine");
        assert!(ps[0].space.is_none());
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
        let press = include_bytes!("../assets/sounds/mx-blue/press.wav");
        let mut f = File::create(pack.join("press.wav")).unwrap();
        f.write_all(press).unwrap();
        let (allp, warns) = all(Some(td.path()));
        assert_eq!(allp.len(), builtins().len());
        assert!(warns.iter().any(|w| w.contains("conflicts")));
    }

    #[test]
    fn missing_user_dir_is_fine() {
        let (ps, warns) = load_user(Path::new("/nonexistent/brokey"));
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 0);
    }
}
