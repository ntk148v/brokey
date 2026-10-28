use crate::download;
use crate::pack::{LoadedPack, PlaybackClip};
use crate::profiles::{Profile, ProfileSource};
use crate::Notice;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStreamBuilder, Source};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(u16),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
    PackInstalled { index: usize, ok: bool },
}

fn load(profile: &Profile, packs: Option<&Path>) -> Result<LoadedPack, String> {
    match &profile.source {
        ProfileSource::Builtin { sprite, manifest } => LoadedPack::from_builtin(sprite, manifest),
        ProfileSource::Custom { press, space } => {
            LoadedPack::from_custom(press.clone(), space.clone())
        }
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
    selected: usize,
    volume: u8,
    paused: bool,
    pack: Option<LoadedPack>,
    downloading: Option<String>,
    pending: Option<usize>,
    preview_on_install: bool,
}

impl State {
    fn new(
        profiles: Vec<Profile>,
        packs_dir: Option<PathBuf>,
        notices: Sender<Notice>,
        tx: Sender<AudioCmd>,
        volume: u8,
    ) -> Self {
        Self {
            profiles,
            packs_dir,
            notices,
            tx,
            selected: 0,
            volume,
            paused: false,
            pack: None,
            downloading: None,
            pending: None,
            preview_on_install: false,
        }
    }

    /// Returns true when the caller should play a preview now.
    fn select(&mut self, index: usize, preview: bool) -> bool {
        let Some(profile) = self.profiles.get(index) else {
            return false;
        };
        let name = profile.name.clone();
        let id = profile.id.clone();
        let optional = profile.is_optional();
        self.selected = index;
        self.preview_on_install = preview;

        if !optional {
            self.pending = None;
            return match load(&self.profiles[index], self.packs_dir.as_deref()) {
                Ok(pack) => {
                    self.pack = Some(pack);
                    preview
                }
                Err(e) => {
                    self.warn(format!("Could not load '{name}': {e}"));
                    false
                }
            };
        }

        let Some(packs) = self.packs_dir.clone() else {
            self.warn(format!("Could not load '{name}': no config directory"));
            return false;
        };

        match download::load_cached(&packs, &id) {
            Ok(pack) => {
                self.pending = None;
                self.pack = Some(pack);
                preview
            }
            Err(e) => {
                if download::is_installed(&packs, &id) {
                    let _ = std::fs::remove_dir_all(download::cache_dir(&packs, &id));
                    self.warn(format!("Refreshing '{name}': {e}"));
                }
                if self.downloading.is_some() {
                    self.pending = Some(index);
                } else {
                    self.start_download(index, packs);
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
            self.pending = None;
            return false;
        }

        let _ = self.notices.send(Notice::Downloaded(id.clone()));
        let mut preview = false;
        if index == self.selected {
            let name = self
                .profiles
                .get(index)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            match self
                .packs_dir
                .as_deref()
                .and_then(|p| download::load_cached(p, &id).ok())
            {
                Some(pack) => {
                    self.pack = Some(pack);
                    preview = self.preview_on_install;
                }
                None => self.warn(format!("Could not load '{name}'")),
            }
        }

        if let Some(next) = self.pending.take() {
            if next == self.selected
                && self
                    .profiles
                    .get(next)
                    .map(Profile::is_optional)
                    .unwrap_or(false)
                && !self.is_cached(next)
            {
                if let Some(packs) = self.packs_dir.clone() {
                    self.start_download(next, packs);
                }
            }
        }
        preview
    }

    fn start_download(&mut self, index: usize, packs: PathBuf) {
        let Some(id) = self.profiles.get(index).map(|p| p.id.clone()) else {
            return;
        };
        self.downloading = Some(id.clone());
        let _ = self.notices.send(Notice::Downloading(id.clone()));
        let tx = self.tx.clone();
        let notices = self.notices.clone();
        std::thread::spawn(move || match download::fetch_and_install(&packs, &id) {
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

    fn is_cached(&self, index: usize) -> bool {
        match (self.profiles.get(index), self.packs_dir.as_deref()) {
            (Some(p), Some(dir)) => !p.is_optional() || download::is_installed(dir, &p.id),
            _ => false,
        }
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
    selected: usize,
    volume: u8,
    notices: Sender<Notice>,
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

        let mut state = State::new(profiles, packs_dir, notices, tx_for_state, volume);
        state.select(selected, false);

        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(scan_code) => state.sound(&stream, scan_code),
                AudioCmd::Preview => state.preview(&stream),
                AudioCmd::SetProfile(i) => {
                    if state.select(i, true) {
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
        SamplesBuffer::new(clip.channels, clip.rate, clip.data()).amplify(volume as f32 / 100.0),
    );
}

fn drain_commands(rx: Receiver<AudioCmd>) {
    while rx.recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Receiver;
    use tempfile::tempdir;

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

    fn state(profiles: Vec<Profile>, packs: &std::path::Path) -> (State, Receiver<crate::Notice>) {
        let (ntx, nrx) = std::sync::mpsc::channel();
        let (tx, _trx) = std::sync::mpsc::channel();
        (
            State::new(profiles, Some(packs.to_path_buf()), ntx, tx, 60),
            nrx,
        )
    }

    #[test]
    fn select_cached_optional_loads_and_previews() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, _nrx) = state(profiles, td.path());
        assert!(st.select(idx, true));
        assert!(st.pack.is_some());
        assert_eq!(st.selected, idx);
    }

    #[test]
    fn installed_success_loads_selected_and_reports() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, nrx) = state(profiles, td.path());
        st.selected = idx;
        st.downloading = Some("holy-panda".to_string());
        st.preview_on_install = true;
        assert!(st.installed(idx, true));
        assert!(st.pack.is_some());
        assert!(st.downloading.is_none());
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloaded(id)) if id == "holy-panda"));
    }

    #[test]
    fn installed_failure_keeps_previous_pack_and_clears_pending() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, _nrx) = state(profiles, td.path());
        assert!(!st.select(0, false));
        assert!(st.pack.is_some());
        st.pending = Some(idx);
        st.downloading = Some("holy-panda".to_string());
        assert!(!st.installed(idx, false));
        assert!(st.pack.is_some());
        assert!(st.pending.is_none());
        assert!(st.downloading.is_none());
    }

    #[test]
    fn installed_when_not_selected_does_not_replace_pack() {
        let (td, profiles, idx) = state_fixture();
        let (mut st, nrx) = state(profiles, td.path());
        st.selected = 0;
        st.downloading = Some("holy-panda".to_string());
        st.preview_on_install = true;
        assert!(!st.installed(idx, true));
        assert!(st.pack.is_none());
        assert!(matches!(nrx.try_recv(), Ok(crate::Notice::Downloaded(id)) if id == "holy-panda"));
    }
}
