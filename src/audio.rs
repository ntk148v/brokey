use crate::listener::SoundKind;
use crate::profiles::Profile;
use crate::Notice;
use rodio::{Decoder, OutputStreamBuilder, Source};
use std::io::Cursor;
use std::sync::mpsc::{Receiver, Sender};

pub type Clip = rodio::source::Buffered<Decoder<Cursor<Vec<u8>>>>;

pub fn decode(bytes: Vec<u8>) -> Result<Clip, String> {
    Decoder::new(Cursor::new(bytes))
        .map(|d| d.buffered())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    Sound(SoundKind),
    Preview,
    SetProfile(usize),
    SetVolume(u8),
    SetPaused(bool),
}

pub fn spawn(profiles: Vec<Profile>, selected: usize, volume: u8, notices: Sender<Notice>) -> Sender<AudioCmd> {
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
        let mut selected_idx = selected;
        let mut volume_val = volume;
        let mut paused = false;
        while let Ok(cmd) = rx.recv() {
            match cmd {
                AudioCmd::Sound(k) => {
                    if paused {
                        continue;
                    }
                    let profile = match profiles.get(selected_idx) {
                        Some(p) => p,
                        None => continue,
                    };
                    let clip = match k {
                        SoundKind::Space => profile.space.as_ref().unwrap_or(&profile.press).clone(),
                        SoundKind::Press => profile.press.clone(),
                    };
                    stream.mixer().add(clip.amplify(volume_val as f32 / 100.0));
                }
                AudioCmd::Preview => {
                    let profile = match profiles.get(selected_idx) {
                        Some(p) => p,
                        None => continue,
                    };
                    stream
                        .mixer()
                        .add(profile.press.clone().amplify(volume_val as f32 / 100.0));
                }
                AudioCmd::SetProfile(i) => {
                    if i < profiles.len() {
                        selected_idx = i;
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

fn drain_commands(rx: Receiver<AudioCmd>) {
    while rx.recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode(b"nope".to_vec()).is_err());
    }

    #[test]
    fn decode_accepts_builtin() {
        assert!(decode(include_bytes!("../assets/sounds/mx-blue/press.wav").to_vec()).is_ok());
    }
}
