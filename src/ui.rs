use crate::audio::AudioCmd;
use crate::config::Config;
use crate::Notice;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

#[derive(Clone)]
pub struct App {
    pub profiles: Vec<(String, String)>, // (id, name)
    pub selected: usize,
    pub volume: u8,
    pub paused: bool,
    pub listener_error: Option<String>,
    pub notice: Option<String>,
    pub quit: bool,
}

pub fn new(profiles: Vec<(String, String)>, cfg: &Config) -> App {
    let mut notice: Option<String> = None;
    let selected_idx_res = if !profiles.is_empty() {
        if let Some(idx) = profiles.iter().position(|(id, _)| id == &cfg.profile) {
            idx
        } else {
            if let Some((_, name)) = profiles.first() {
                notice = Some(format!(
                    "Saved profile '{}' not found; using {}",
                    cfg.profile, name
                ));
            } else {
                notice = Some(format!("Saved profile '{}' not found", cfg.profile));
            }
            0
        }
    } else {
        0
    };
    App {
        profiles,
        selected: selected_idx_res,
        volume: cfg.volume,
        paused: false,
        listener_error: None,
        notice,
        quit: false,
    }
}

pub fn on_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Option<AudioCmd> {
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.profiles.is_empty() {
                return None;
            }
            if app.selected == 0 {
                return None;
            }
            let old = app.selected;
            app.selected -= 1;
            if app.selected != old {
                return Some(AudioCmd::SetProfile(app.selected));
            }
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.profiles.is_empty() {
                return None;
            }
            if app.selected >= app.profiles.len() - 1 {
                return None;
            }
            let old = app.selected;
            app.selected += 1;
            if app.selected != old {
                return Some(AudioCmd::SetProfile(app.selected));
            }
            None
        }
        KeyCode::Left | KeyCode::Char('-') => {
            let old = app.volume;
            app.volume = app.volume.saturating_sub(10);
            if app.volume != old {
                Some(AudioCmd::SetVolume(app.volume))
            } else {
                None
            }
        }
        KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
            let old = app.volume;
            app.volume = ((app.volume as u16) + 10).min(100) as u8;
            if app.volume != old {
                Some(AudioCmd::SetVolume(app.volume))
            } else {
                None
            }
        }
        KeyCode::Char(' ') => {
            app.paused = !app.paused;
            Some(AudioCmd::SetPaused(app.paused))
        }
        KeyCode::Char('p') => Some(AudioCmd::Preview),
        KeyCode::Char('q') => {
            app.quit = true;
            None
        }
        KeyCode::Esc => {
            app.quit = true;
            None
        }
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
            app.quit = true;
            None
        }
        _ => None,
    }
}

impl App {
    pub fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) -> Option<AudioCmd> {
        on_key(self, code, mods)
    }

    pub fn config(&self) -> Config {
        if self.profiles.is_empty() {
            return Config::default();
        }
        Config {
            profile: self.profiles[self.selected].0.clone(),
            volume: self.volume,
        }
    }

    pub fn on_notice(&mut self, n: Notice) {
        match n {
            Notice::ListenerFailed(e) => {
                self.listener_error = Some(e);
            }
            Notice::Warn(e) => {
                self.notice = Some(e);
            }
        }
    }
}

pub fn on_notice(app: &mut App, n: Notice) {
    app.on_notice(n);
}

pub fn config(app: &App) -> Config {
    app.config()
}

pub fn draw(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    let content_area = if area.width >= 4 && area.height >= 3 {
        area.inner(Margin {
            horizontal: 1,
            vertical: 0,
        })
    } else {
        area
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(content_area);

    let title = Paragraph::new("Brokey — Real thock. Fake keyboard. Zero budget.").style(
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(title, chunks[0]);

    let status_text = if let Some(err) = &app.listener_error {
        format!("✗ Not listening: {}", err)
    } else if app.paused {
        "⏸ Paused".to_string()
    } else {
        "● Listening".to_string()
    };
    let status_style = if app.listener_error.is_some() {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if app.paused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };
    let status = Paragraph::new(status_text).style(status_style);
    frame.render_widget(status, chunks[1]);

    let gauge_color = if app.volume >= 70 {
        Color::Green
    } else if app.volume >= 30 {
        Color::Cyan
    } else {
        Color::LightYellow
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Volume {}%", app.volume))
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .gauge_style(
            Style::default()
                .fg(gauge_color)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .use_unicode(true)
        .ratio((app.volume as f64) / 100.0);
    frame.render_widget(gauge, chunks[2]);

    let items: Vec<ListItem> = app
        .profiles
        .iter()
        .enumerate()
        .map(|(idx, (_id, name))| {
            if idx == app.selected {
                ListItem::new(format!("▸ {}", name.as_str())).style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(format!("  {}", name.as_str()))
            }
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Profiles")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    let mut state = ratatui::widgets::ListState::default();
    if !app.profiles.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, chunks[3], &mut state);

    if let Some(notice) = &app.notice {
        let notice_para = Paragraph::new(notice.as_str()).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(notice_para, chunks[4]);
    } else {
        let empty = Paragraph::new("");
        frame.render_widget(empty, chunks[4]);
    }

    let footer = Paragraph::new("↑↓ profile  ←→ volume  space pause  p preview  q quit")
        .style(Style::default().fg(Color::Reset));
    frame.render_widget(footer, chunks[5]);
}

pub fn run(
    mut app: App,
    audio: Sender<AudioCmd>,
    notices: Receiver<Notice>,
    cfg_path: Option<PathBuf>,
) -> io::Result<()> {
    let mut terminal = ratatui::init();
    loop {
        terminal.draw(|f| draw(f, &app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let cmd = on_key(&mut app, key.code, key.modifiers);
                    if let Some(c) = cmd {
                        let is_set_profile = matches!(c, AudioCmd::SetProfile(_));
                        let _ = audio.send(c);
                        if is_set_profile {
                            let _ = audio.send(AudioCmd::Preview);
                        }
                        if matches!(c, AudioCmd::SetProfile(_) | AudioCmd::SetVolume(_)) {
                            if let Some(ref p) = cfg_path {
                                if let Err(e) = crate::config::save(p, &App::config(&app)) {
                                    app.notice = Some(e);
                                }
                            }
                        }
                    }
                }
            }
        }
        while let Ok(n) = notices.try_recv() {
            on_notice(&mut app, n);
        }
        if app.quit {
            break;
        }
    }
    ratatui::restore();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn app() -> App {
        let profiles = vec![
            ("mx-blue".to_string(), "Cherry MX Blue".to_string()),
            ("mx-red".to_string(), "Cherry MX Red".to_string()),
            ("mx-brown".to_string(), "Cherry MX Brown".to_string()),
            ("topre".to_string(), "Topre".to_string()),
        ];
        new(profiles, &Config::default())
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let mut s = String::new();
        let area = buf.area;
        for y in 0..area.height {
            for x in 0..area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn new_falls_back_when_profile_missing() {
        let profiles = app().profiles.clone();
        let cfg = Config {
            profile: "gone".into(),
            volume: 60,
        };
        let a = new(profiles, &cfg);
        assert_eq!(a.selected, 0);
        assert!(a.notice.as_ref().unwrap().contains("gone"));
    }

    #[test]
    fn new_selects_saved_profile() {
        let profiles = app().profiles.clone();
        let cfg = Config {
            profile: "topre".into(),
            volume: 60,
        };
        let a = new(profiles, &cfg);
        assert_eq!(a.selected, 3);
    }

    #[test]
    fn down_switches_profile() {
        let mut a = app();
        assert_eq!(
            on_key(&mut a, KeyCode::Down, KeyModifiers::NONE),
            Some(AudioCmd::SetProfile(1))
        );
        // move to last
        let mut a = app();
        a.selected = 3;
        assert_eq!(on_key(&mut a, KeyCode::Down, KeyModifiers::NONE), None);
    }

    #[test]
    fn volume_steps_and_clamps() {
        let mut a = app();
        assert_eq!(a.volume, 60);
        // 4 times to reach 100
        assert_eq!(
            on_key(&mut a, KeyCode::Right, KeyModifiers::NONE),
            Some(AudioCmd::SetVolume(70))
        );
        assert_eq!(
            on_key(&mut a, KeyCode::Right, KeyModifiers::NONE),
            Some(AudioCmd::SetVolume(80))
        );
        assert_eq!(
            on_key(&mut a, KeyCode::Right, KeyModifiers::NONE),
            Some(AudioCmd::SetVolume(90))
        );
        assert_eq!(
            on_key(&mut a, KeyCode::Right, KeyModifiers::NONE),
            Some(AudioCmd::SetVolume(100))
        );
        assert_eq!(on_key(&mut a, KeyCode::Right, KeyModifiers::NONE), None);
        // left
        let mut a = app();
        for _ in 0..10 {
            let _ = on_key(&mut a, KeyCode::Left, KeyModifiers::NONE);
        }
        assert_eq!(a.volume, 0);
        assert_eq!(on_key(&mut a, KeyCode::Left, KeyModifiers::NONE), None);
    }

    #[test]
    fn space_toggles_pause() {
        let mut a = app();
        assert_eq!(
            on_key(&mut a, KeyCode::Char(' '), KeyModifiers::NONE),
            Some(AudioCmd::SetPaused(true))
        );
        assert_eq!(
            on_key(&mut a, KeyCode::Char(' '), KeyModifiers::NONE),
            Some(AudioCmd::SetPaused(false))
        );
    }

    #[test]
    fn p_previews() {
        let mut a = app();
        assert_eq!(
            on_key(&mut a, KeyCode::Char('p'), KeyModifiers::NONE),
            Some(AudioCmd::Preview)
        );
    }

    #[test]
    fn quit_keys() {
        let mut a = app();
        on_key(&mut a, KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(a.quit);
        let mut a = app();
        on_key(&mut a, KeyCode::Esc, KeyModifiers::NONE);
        assert!(a.quit);
        let mut a = app();
        on_key(&mut a, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(a.quit);
    }

    #[test]
    fn renders_profile_and_volume() {
        let a = app();
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Cherry MX Blue"));
        assert!(text.contains("Volume 60%"));
        assert!(text.contains("Listening"));
    }

    #[test]
    fn renders_listener_error() {
        let mut a = app();
        on_notice(&mut a, Notice::ListenerFailed("denied".into()));
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Not listening: denied"));
    }

    #[test]
    fn renders_in_tiny_terminal() {
        let a = app();
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
    }
}
