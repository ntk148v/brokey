use std::sync::mpsc;
use std::thread;

pub mod audio;
pub mod config;
pub mod listener;
pub mod pack;
pub mod profiles;
pub mod ui;

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    ListenerFailed(String),
    Warn(String),
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
    let (profiles_vec, warnings) = profiles::all(user_dir.as_deref());
    let profile_pairs: Vec<(String, String)> = profiles_vec
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    let mut app = ui::new(profile_pairs.clone(), &cfg);
    if let Some(w) = cfg_warn {
        crate::ui::on_notice(&mut app, Notice::Warn(w));
    }
    for w in warnings {
        crate::ui::on_notice(&mut app, Notice::Warn(w));
    }

    let (notice_tx, notice_rx) = mpsc::channel::<Notice>();
    let audio_tx = audio::spawn(profiles_vec, app.selected, app.volume, notice_tx.clone());

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
