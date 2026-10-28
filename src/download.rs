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
        return Err(format!(
            "GET {url}: response is larger than {MAX_BYTES} bytes"
        ));
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
    Manifest::parse(manifest).map_err(|e| format!("pack '{id}' has an invalid manifest: {e}"))?;
    LoadedPack::from_builtin(sprite, manifest)
        .map_err(|e| format!("pack '{id}' has invalid audio: {e}"))?;

    fs::create_dir_all(packs).map_err(|e| format!("could not create {}: {e}", packs.display()))?;

    let unique = unique_suffix();
    let tmp = packs.join(format!(".tmp-{id}-{unique}"));
    let backup = packs.join(format!(".old-{id}-{unique}"));
    let target = cache_dir(packs, id);

    let installed = (|| -> Result<(), String> {
        fs::create_dir_all(&tmp).map_err(|e| format!("could not create {}: {e}", tmp.display()))?;
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
        for id in ["", "../x", "MX-Blue", "a/b", ".hidden", "a b", "-leading"] {
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
