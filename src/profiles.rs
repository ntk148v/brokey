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

pub fn builtins() -> Vec<Profile> {
    let p1_press = decode(include_bytes!("../assets/sounds/mx-blue/press.wav").to_vec())
        .expect("built-in press.wav must decode");
    let p1_space = decode(include_bytes!("../assets/sounds/mx-blue/space.wav").to_vec())
        .expect("built-in space.wav must decode");
    let p2_press = decode(include_bytes!("../assets/sounds/mx-red/press.wav").to_vec())
        .expect("built-in press.wav must decode");
    let p2_space = decode(include_bytes!("../assets/sounds/mx-red/space.wav").to_vec())
        .expect("built-in space.wav must decode");
    let p3_press = decode(include_bytes!("../assets/sounds/mx-brown/press.wav").to_vec())
        .expect("built-in press.wav must decode");
    let p3_space = decode(include_bytes!("../assets/sounds/mx-brown/space.wav").to_vec())
        .expect("built-in space.wav must decode");
    let p4_press = decode(include_bytes!("../assets/sounds/topre/press.wav").to_vec())
        .expect("built-in press.wav must decode");
    let p4_space = decode(include_bytes!("../assets/sounds/topre/space.wav").to_vec())
        .expect("built-in space.wav must decode");
    vec![
        Profile {
            id: "mx-blue".to_string(),
            name: "Cherry MX Blue".to_string(),
            press: p1_press,
            space: Some(p1_space),
        },
        Profile {
            id: "mx-red".to_string(),
            name: "Cherry MX Red".to_string(),
            press: p2_press,
            space: Some(p2_space),
        },
        Profile {
            id: "mx-brown".to_string(),
            name: "Cherry MX Brown".to_string(),
            press: p3_press,
            space: Some(p3_space),
        },
        Profile {
            id: "topre".to_string(),
            name: "Topre".to_string(),
            press: p4_press,
            space: Some(p4_space),
        },
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
    fn builtins_are_the_four_switches() {
        let b = builtins();
        assert_eq!(b.len(), 4);
        let ids: Vec<String> = b.iter().map(|p| p.id.clone()).collect();
        assert_eq!(
            ids,
            vec![
                "mx-blue".to_string(),
                "mx-red".to_string(),
                "mx-brown".to_string(),
                "topre".to_string()
            ]
        );
        assert_eq!(b[0].name, "Cherry MX Blue");
        assert_eq!(b[1].name, "Cherry MX Red");
        assert_eq!(b[2].name, "Cherry MX Brown");
        assert_eq!(b[3].name, "Topre");
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
        assert_eq!(allp.len(), 4);
        assert!(warns.iter().any(|w| w.contains("conflicts")));
    }

    #[test]
    fn missing_user_dir_is_fine() {
        let (ps, warns) = load_user(Path::new("/nonexistent/brokey"));
        assert_eq!(ps.len(), 0);
        assert_eq!(warns.len(), 0);
    }
}
