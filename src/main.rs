use std::sync::mpsc;

pub mod audio;
pub mod config;
pub mod listener;
pub mod profiles;

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    ListenerFailed(String),
    Warn(String),
}

fn main() {
    let (ntx, nrx) = mpsc::channel();
    let profiles = profiles::builtins();
    let audio = audio::spawn(profiles, 0, 60, ntx.clone());
    std::thread::spawn(move || {
        while let Ok(n) = nrx.recv() {
            eprintln!("{:?}", n);
        }
    });

    println!("brokey: listening — type anywhere, Ctrl-C to quit");
    if let Err(e) = listener::listen(move |k| {
        let _ = audio.send(audio::AudioCmd::Sound(k));
    }) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
