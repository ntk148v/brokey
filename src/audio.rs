use crate::download;
use crate::pack::{LoadedPack, PlaybackClip};
use crate::profiles::{Profile, ProfileSource};
use crate::Notice;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStreamBuilder, Source};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

type Fetcher = fn(&Path, &str) -> Result<(), String>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(u16),
    Release,
    Preview,
    Activate(usize),
    SetVolume(u8),
    SetPaused(bool),
    PackInstalled { index: usize, ok: bool },
}

fn load(profile: &Profile, packs: Option<&Path>) -> Result<LoadedPack, String> {
    match &profile.source {
        ProfileSource::Builtin { sprite, manifest } => LoadedPack::from_builtin(sprite, manifest),
        ProfileSource::Custom {
            press,
            space,
            release,
        } => LoadedPack::from_custom(press.clone(), space.clone(), release.clone()),
        ProfileSource::Sprite { sprite, manifest } => LoadedPack::from_builtin(sprite, manifest),
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
    volume: u8,
    paused: bool,
    pack: Option<LoadedPack>,
    downloading: Option<String>,
    fetch: Fetcher,
}

impl State {
    fn new(
        profiles: Vec<Profile>,
        packs_dir: Option<PathBuf>,
        notices: Sender<Notice>,
        tx: Sender<AudioCmd>,
        volume: u8,
        fetch: Fetcher,
    ) -> Self {
        Self {
            profiles,
            packs_dir,
            notices,
            tx,
            volume,
            paused: false,
            pack: None,
            downloading: None,
            fetch,
        }
    }

    fn start(&mut self, saved: usize) {
        if self.profiles.is_empty() {
            return;
        }
        let Some(profile) = self.profiles.get(saved) else {
            self.fallback(format!("Saved profile index {saved} not found"));
            return;
        };
        let name = profile.name.clone();

        if !profile.is_available(self.packs_dir.as_deref()) {
            self.fallback(format!(
                "Saved profile '{name}' needs downloading; press Enter to download"
            ));
            return;
        }

        match load(&self.profiles[saved], self.packs_dir.as_deref()) {
            Ok(pack) => {
                self.pack = Some(pack);
                let _ = self.notices.send(Notice::Activated {
                    index: saved,
                    user: false,
                });
            }
            Err(e) => self.fallback(format!("Could not load '{name}': {e}")),
        }
    }

    fn fallback(&mut self, reason: String) {
        self.warn(reason);
        if self.profiles.is_empty() {
            return;
        }
        let index = self
            .profiles
            .iter()
            .position(|p| !p.is_optional())
            .unwrap_or(0);
        let name = self.profiles[index].name.clone();
        match load(&self.profiles[index], self.packs_dir.as_deref()) {
            Ok(pack) => {
                self.pack = Some(pack);
                let _ = self.notices.send(Notice::Activated { index, user: false });
            }
            Err(e) => self.warn(format!("Could not load fallback '{name}': {e}")),
        }
    }

    /// Returns true when the caller should play a preview now.
    fn activate(&mut self, index: usize) -> bool {
        let Some(profile) = self.profiles.get(index) else {
            self.warn(format!("Profile {index} not found"));
            return false;
        };
        let name = profile.name.clone();
        let id = profile.id.clone();
        let optional = profile.is_optional();

        if let Some(current) = self.downloading.clone() {
            let current_name = self
                .profiles
                .iter()
                .find(|p| p.id == current)
                .map(|p| p.name.clone())
                .unwrap_or(current);
            self.warn(format!("Download in progress: {current_name}"));
            return false;
        }

        match load(&self.profiles[index], self.packs_dir.as_deref()) {
            Ok(pack) => {
                self.pack = Some(pack);
                let _ = self.notices.send(Notice::Activated { index, user: true });
                true
            }
            Err(e) => {
                match self.packs_dir.clone() {
                    Some(packs) if optional => {
                        if download::is_installed(&packs, &id) {
                            let _ = std::fs::remove_dir_all(download::cache_dir(&packs, &id));
                            self.warn(format!("Refreshing '{name}': {e}"));
                        }
                        self.start_download(index, packs);
                    }
                    _ => self.warn(format!("Could not load '{name}': {e}")),
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
            return false;
        }

        let _ = self.notices.send(Notice::Downloaded(id.clone()));
        let Some(profile) = self.profiles.get(index) else {
            return false;
        };
        let name = profile.name.clone();
        match self
            .packs_dir
            .as_deref()
            .and_then(|p| download::load_cached(p, &id).ok())
        {
            Some(pack) => {
                self.pack = Some(pack);
                let _ = self.notices.send(Notice::Activated { index, user: true });
                true
            }
            None => {
                self.warn(format!("Could not load '{name}'"));
                false
            }
        }
    }

    fn start_download(&mut self, index: usize, packs: PathBuf) {
        let Some(id) = self.profiles.get(index).map(|p| p.id.clone()) else {
            return;
        };
        self.downloading = Some(id.clone());
        let _ = self.notices.send(Notice::Downloading(id.clone()));
        let tx = self.tx.clone();
        let notices = self.notices.clone();
        let fetch = self.fetch;
        std::thread::spawn(move || match fetch(&packs, &id) {
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

    fn release(&mut self, stream: &rodio::OutputStream) {
        if self.paused {
            return;
        }
        if let Some(clip) = self.pack.as_mut().and_then(|p| p.next_release()) {
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
    saved: usize,
    volume: u8,
    notices: Sender<Notice>,
) -> Sender<AudioCmd> {
    spawn_with_fetcher(
        profiles,
        packs_dir,
        saved,
        volume,
        notices,
        download::fetch_and_install,
    )
}

fn spawn_with_fetcher(
    profiles: Vec<Profile>,
    packs_dir: Option<PathBuf>,
    saved: usize,
    volume: u8,
    notices: Sender<Notice>,
    fetch: Fetcher,
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

        let mut state = State::new(profiles, packs_dir, notices, tx_for_state, volume, fetch);
        state.start(saved);

        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(scan_code) => state.sound(&stream, scan_code),
                AudioCmd::Release => state.release(&stream),
                AudioCmd::Preview => state.preview(&stream),
                AudioCmd::Activate(i) => {
                    if state.activate(i) {
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

fn play(stream: &rodio::OutputStream, clip: &PlaybackClip, volume: u8) {
    stream.mixer().add(
        SamplesBuffer::new(clip.channels, clip.rate, clip.data())
            .speed(jitter())
            .amplify(volume as f32 / 100.0),
    );
}

/// Speed factor in 0.97..=1.03 so repeated keys don't sound identical.
/// Keystroke timing is human, so clock nanoseconds are random enough.
fn jitter() -> f32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    1.0 + ((nanos % 61) as f32 - 30.0) / 1000.0
}

fn drain_commands(rx: Receiver<AudioCmd>) {
    while rx.recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    use tempfile::tempdir;

    fn ok_fetch(_: &Path, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn slow_fetch(_: &Path, _: &str) -> Result<(), String> {
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(())
    }

    fn fail_fetch(_: &Path, _: &str) -> Result<(), String> {
        Err("stub failure".to_string())
    }

    #[test]
    fn builtin_profile_loads() {
        let (profiles, warnings) = crate::profiles::builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!profiles.is_empty());
        assert!(load(&profiles[0], None).is_ok());
    }

    #[test]
    fn custom_profile_loads() {
        let wav = crate::testutil::wav_bytes(&[0, 1000, -1000, 0]);
        let profile = Profile {
            id: "mine".to_string(),
            name: "mine".to_string(),
            source: ProfileSource::Custom {
                press: wav,
                space: None,
                release: None,
            },
        };
        assert!(load(&profile, None).is_ok());
    }

    fn state_fixture() -> (tempfile::TempDir, Vec<Profile>, usize) {
        let td = tempdir().unwrap();
        let (sprite, manifest) = crate::profiles::tests::embedded_bytes();
        crate::download::install_from_bytes(td.path(), "holy-panda", manifest, sprite).unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
        (td, profiles, idx)
    }

    fn state(
        profiles: Vec<Profile>,
        packs: &Path,
        fetch: Fetcher,
    ) -> (State, Receiver<crate::Notice>) {
        let (ntx, nrx) = std::sync::mpsc::channel();
        let (tx, _trx) = std::sync::mpsc::channel();
        (
            State::new(profiles, Some(packs.to_path_buf()), ntx, tx, 60, fetch),
            nrx,
        )
    }

    #[test]
    fn start_default_activates() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.start(0);
        assert!(st.pack.is_some());
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated {
                index: 0,
                user: false
            })
        ));
    }

    #[test]
    fn start_uncached_optional_falls_back_without_download() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.start(idx);
        assert!(st.pack.is_some());
        assert!(st.downloading.is_none());
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Warn(_))));
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated {
                index: 0,
                user: false
            })
        ));
    }

    #[test]
    fn start_cached_optional_activates() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.start(idx);
        assert!(st.pack.is_some());
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated { index, user: false }) if index == idx
        ));
    }

    #[test]
    fn start_corrupt_optional_falls_back() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
        let dir = crate::download::cache_dir(td.path(), "holy-panda");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        std::fs::write(dir.join("sound.flac"), b"nope").unwrap();

        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.start(idx);
        assert!(st.pack.is_some());
        assert!(st.downloading.is_none());
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Warn(_))));
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated {
                index: 0,
                user: false
            })
        ));
    }

    #[test]
    fn activate_embedded_loads_for_user() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        assert!(st.activate(0));
        assert!(st.pack.is_some());
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated {
                index: 0,
                user: true
            })
        ));
    }

    #[test]
    fn activate_cached_optional_loads_for_user() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        assert!(st.activate(idx));
        assert!(st.pack.is_some());
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated { index, user: true }) if index == idx
        ));
    }

    #[test]
    fn activate_uncached_optional_starts_download() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
        let (mut st, nrx) = state(profiles, td.path(), slow_fetch);
        assert!(!st.activate(idx));
        assert_eq!(st.downloading.as_deref(), Some("holy-panda"));
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloading(id)) if id == "holy-panda"));
    }

    #[test]
    fn activate_corrupt_optional_removes_cache_and_downloads() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let idx = profiles.iter().position(|p| p.id == "holy-panda").unwrap();
        let dir = crate::download::cache_dir(td.path(), "holy-panda");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        std::fs::write(dir.join("sound.flac"), b"nope").unwrap();

        let (mut st, _nrx) = state(profiles, td.path(), fail_fetch);
        assert!(!st.activate(idx));
        assert!(!dir.exists());
        assert_eq!(st.downloading.as_deref(), Some("holy-panda"));
    }

    #[test]
    fn activate_while_downloading_is_rejected() {
        let td = tempdir().unwrap();
        let (profiles, _) = crate::profiles::all(None);
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.downloading = Some("holy-panda".to_string());
        assert!(!st.activate(0));
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Warn(msg)) if msg.contains("Download in progress")
        ));
    }

    #[test]
    fn installed_success_activates_for_user() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, nrx) = state(profiles, td.path(), ok_fetch);
        st.downloading = Some("holy-panda".to_string());
        assert!(st.installed(idx, true));
        assert!(st.pack.is_some());
        assert!(st.downloading.is_none());
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloaded(id)) if id == "holy-panda"));
        assert!(matches!(
            nrx.try_recv(),
            Ok(crate::Notice::Activated { index, user: true }) if index == idx
        ));
    }

    #[test]
    fn installed_failure_keeps_previous_pack() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, _nrx) = state(profiles, td.path(), ok_fetch);
        st.start(0);
        assert!(st.pack.is_some());
        st.downloading = Some("holy-panda".to_string());
        assert!(!st.installed(idx, false));
        assert!(st.pack.is_some());
        assert!(st.downloading.is_none());
    }
}
