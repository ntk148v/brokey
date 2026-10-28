use crate::pack::{LoadedPack, PlaybackClip};
use crate::profiles::{Profile, ProfileSource};
use crate::Notice;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStreamBuilder, Source};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(u16),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
}

fn load(profile: &Profile) -> Result<LoadedPack, String> {
    match &profile.source {
        ProfileSource::Builtin { sprite, manifest } => LoadedPack::from_builtin(sprite, manifest),
        ProfileSource::Custom { press, space } => {
            LoadedPack::from_custom(press.clone(), space.clone())
        }
    }
}

pub fn spawn(
    profiles: Vec<Profile>,
    selected: usize,
    volume: u8,
    notices: Sender<Notice>,
) -> Sender<AudioCmd> {
    let (tx, rx) = std::sync::mpsc::channel::<AudioCmd>();
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
        let mut volume_val = volume;
        let mut paused = false;
        let mut pack = profiles
            .get(selected)
            .and_then(|profile| match load(profile) {
                Ok(pack) => Some(pack),
                Err(e) => {
                    let _ = notices.send(Notice::Warn(format!(
                        "Could not load '{}': {e}",
                        profile.name
                    )));
                    None
                }
            });

        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(scan_code) => {
                    if paused {
                        continue;
                    }
                    if let Some(pack) = pack.as_mut() {
                        let clip = pack.clip_for(scan_code);
                        play(&stream, &clip, volume_val);
                    }
                }
                AudioCmd::Preview => {
                    if let Some(pack) = pack.as_mut() {
                        let clip = pack.next_default();
                        play(&stream, &clip, volume_val);
                    }
                }
                AudioCmd::SetProfile(i) => {
                    let profile = match profiles.get(i) {
                        Some(p) => p,
                        None => continue,
                    };
                    match load(profile) {
                        Ok(new_pack) => {
                            pack = Some(new_pack);
                        }
                        Err(e) => {
                            let _ = notices.send(Notice::Warn(format!(
                                "Could not load '{}': {e}",
                                profile.name
                            )));
                        }
                    }
                }
                AudioCmd::SetVolume(v) => {
                    volume_val = v;
                }
                AudioCmd::SetPaused(p) => {
                    paused = p;
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

    #[test]
    fn builtin_profile_loads() {
        let (profiles, warnings) = crate::profiles::builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!profiles.is_empty());
        assert!(load(&profiles[0]).is_ok());
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
        assert!(load(&profile).is_ok());
    }
}
