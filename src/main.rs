use std::sync::mpsc;
use std::thread;

pub mod audio;
pub mod config;
pub mod download;
pub mod listener;
pub mod pack;
pub mod profiles;
pub mod ui;

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    ListenerFailed(String),
    Warn(String),
    Downloading(String),
    Downloaded(String),
    DownloadFailed { id: String, error: String },
    Activated { index: usize, user: bool },
}

fn main() {
    let cfg_path = config::dir().map(|d| d.join("config.toml"));
    let cfg_path_for_load = cfg_path.clone();
    let (cfg, cfg_warn) = if let Some(ref p) = cfg_path_for_load {
        config::load(p)
    } else {
        (config::Config::default(), None)
    };

    let user_dir = config::dir().map(|d| d.join("sounds"));
    let packs_dir = config::dir().map(|d| d.join("packs"));
    let (profiles_vec, warnings) = profiles::all(user_dir.as_deref());
    let entries: Vec<ui::ProfileEntry> = profiles_vec
        .iter()
        .map(|p| ui::ProfileEntry {
            id: p.id.clone(),
            name: p.name.clone(),
            optional: p.is_optional(),
            available: p.is_available(packs_dir.as_deref()),
            downloading: false,
        })
        .collect();

    let mut app = ui::new(entries, &cfg);
    if let Some(w) = cfg_warn {
        let _ = crate::ui::on_notice(&mut app, Notice::Warn(w));
    }
    for w in warnings {
        let _ = crate::ui::on_notice(&mut app, Notice::Warn(w));
    }

    let (notice_tx, notice_rx) = mpsc::channel::<Notice>();
    let audio_tx = audio::spawn(
        profiles_vec,
        packs_dir,
        app.cursor,
        app.volume,
        notice_tx.clone(),
    );

    let app_for_ui = app;
    let audio_tx_clone = audio_tx.clone();
    let notice_rx_clone = notice_rx;
    let cfg_path_for_ui = cfg_path.clone();
    let ui_handle = thread::spawn(move || {
        let _ = ui::run(app_for_ui, audio_tx_clone, notice_rx_clone, cfg_path_for_ui);
        std::process::exit(0);
    });

    if let Err(e) = listener::listen(move |k| {
        let _ = audio_tx.send(audio::AudioCmd::Sound(k));
    }) {
        // rdev cannot be restarted, so keep the TUI alive: it shows the reason
        // and the user quits with q. The UI thread terminates the process,
        // because this thread stays parked inside `rdev::listen` otherwise.
        let _ = notice_tx.send(Notice::ListenerFailed(e));
        let _ = ui_handle.join();
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + samples.len() * 2);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&44_100u32.to_le_bytes());
        out.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }
}
