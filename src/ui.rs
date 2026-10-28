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

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileEntry {
    pub id: String,
    pub name: String,
    pub optional: bool,
    pub available: bool,
    pub downloading: bool,
}

#[derive(Clone)]
pub struct App {
    pub profiles: Vec<ProfileEntry>,
    pub cursor: usize,
    pub active: Option<usize>,
    pub profile_id: String,
    pub volume: u8,
    pub paused: bool,
    pub listener_error: Option<String>,
    pub notice: Option<String>,
    pub quit: bool,
}

pub fn new(profiles: Vec<ProfileEntry>, cfg: &Config) -> App {
    let (cursor, profile_id, notice) = if profiles.is_empty() {
        (0, cfg.profile.clone(), None)
    } else if let Some(idx) = profiles.iter().position(|p| p.id == cfg.profile) {
        (idx, cfg.profile.clone(), None)
    } else {
        let first = &profiles[0];
        (
            0,
            first.id.clone(),
            Some(format!(
                "Saved profile '{}' not found; using {}",
                cfg.profile, first.name
            )),
        )
    };

    App {
        profiles,
        cursor,
        active: None,
        profile_id,
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
            if app.cursor > 0 {
                app.cursor -= 1;
            }
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !app.profiles.is_empty() && app.cursor < app.profiles.len() - 1 {
                app.cursor += 1;
            }
            None
        }
        KeyCode::Enter => {
            if app.profiles.is_empty() {
                return None;
            }
            if let Some(entry) = app.profiles.iter().find(|p| p.downloading) {
                app.notice = Some(format!("Download in progress: {}", entry.name));
                return None;
            }
            Some(AudioCmd::Activate(app.cursor))
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
        Config {
            profile: self.profile_id.clone(),
            volume: self.volume,
        }
    }

    /// Returns true when the caller should save the config.
    pub fn on_notice(&mut self, n: Notice) -> bool {
        match n {
            Notice::ListenerFailed(e) => {
                self.listener_error = Some(e);
                false
            }
            Notice::Warn(e) => {
                self.notice = Some(e);
                false
            }
            Notice::Downloading(id) => {
                let name = self
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.clone());
                if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
                    entry.downloading = true;
                }
                self.notice = Some(format!("Downloading {name}…"));
                false
            }
            Notice::Downloaded(id) => {
                let name = self
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.clone());
                if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
                    entry.available = true;
                    entry.downloading = false;
                }
                self.notice = Some(format!("Downloaded {name}"));
                false
            }
            Notice::DownloadFailed { id, error } => {
                let name = self
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| id.clone());
                if let Some(entry) = self.profiles.iter_mut().find(|p| p.id == id) {
                    entry.downloading = false;
                }
                self.notice = Some(format!("Could not download {name}: {error}"));
                false
            }
            Notice::Activated { index, user } => {
                if self.profiles.get(index).is_none() {
                    return false;
                }
                self.active = Some(index);
                if !user {
                    return false;
                }
                self.profile_id = self.profiles[index].id.clone();
                self.notice = Some(format!("Loaded {}", self.profiles[index].name));
                true
            }
        }
    }
}

pub fn on_notice(app: &mut App, n: Notice) -> bool {
    app.on_notice(n)
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
        .map(|(idx, entry)| {
            let suffix = if entry.downloading {
                "  …"
            } else if entry.optional && !entry.available {
                "  ↓"
            } else {
                ""
            };
            let cursor_marker = if idx == app.cursor { "▸" } else { " " };
            let active_marker = if app.active == Some(idx) { "♪" } else { " " };
            let text = format!("{cursor_marker}{active_marker} {}{suffix}", entry.name);
            let style = if idx == app.cursor {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if app.active == Some(idx) {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else if entry.downloading {
                Style::default().fg(Color::Yellow)
            } else if entry.optional && !entry.available {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
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
        state.select(Some(app.cursor));
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

    let footer = Paragraph::new("↑↓ browse  enter load  ←→ volume  space pause  p preview  q quit")
        .style(Style::default().fg(Color::Reset));
    frame.render_widget(footer, chunks[5]);
}

fn save(app: &App, cfg_path: &Option<PathBuf>) -> Option<String> {
    cfg_path
        .as_ref()
        .and_then(|p| crate::config::save(p, &app.config()).err())
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
                        let _ = audio.send(c);
                        if matches!(c, AudioCmd::SetVolume(_)) {
                            if let Some(e) = save(&app, &cfg_path) {
                                app.notice = Some(e);
                            }
                        }
                    }
                }
            }
        }
        while let Ok(n) = notices.try_recv() {
            if on_notice(&mut app, n) {
                if let Some(e) = save(&app, &cfg_path) {
                    app.notice = Some(e);
                }
            }
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

    fn entry(id: &str, name: &str, optional: bool, available: bool) -> ProfileEntry {
        ProfileEntry {
            id: id.to_string(),
            name: name.to_string(),
            optional,
            available,
            downloading: false,
        }
    }

    fn app() -> App {
        let profiles = vec![
            entry("mx-blue", "Cherry MX Blue (ABS)", false, true),
            entry("mx-red", "Cherry MX Red (ABS)", false, true),
            entry("mx-brown", "Cherry MX Brown (ABS)", false, true),
            entry("topre", "Topre", false, true),
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
        assert_eq!(a.cursor, 0);
        assert_eq!(a.profile_id, "mx-blue");
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
        assert_eq!(a.cursor, 3);
        assert_eq!(a.profile_id, "topre");
    }

    #[test]
    fn down_moves_cursor_only() {
        let mut a = app();
        assert_eq!(on_key(&mut a, KeyCode::Down, KeyModifiers::NONE), None);
        assert_eq!(a.cursor, 1);
        assert!(a.active.is_none());

        a.cursor = 3;
        assert_eq!(on_key(&mut a, KeyCode::Down, KeyModifiers::NONE), None);
        assert_eq!(a.cursor, 3);
    }

    #[test]
    fn up_moves_cursor_only() {
        let mut a = app();
        a.cursor = 2;
        assert_eq!(on_key(&mut a, KeyCode::Up, KeyModifiers::NONE), None);
        assert_eq!(a.cursor, 1);
        assert!(a.active.is_none());
    }

    #[test]
    fn enter_activates_cursor() {
        let mut a = app();
        assert_eq!(
            on_key(&mut a, KeyCode::Enter, KeyModifiers::NONE),
            Some(AudioCmd::Activate(0))
        );
        a.cursor = 2;
        assert_eq!(
            on_key(&mut a, KeyCode::Enter, KeyModifiers::NONE),
            Some(AudioCmd::Activate(2))
        );
    }

    #[test]
    fn enter_rejected_during_download() {
        let mut a = app();
        a.profiles[1].downloading = true;
        assert_eq!(on_key(&mut a, KeyCode::Enter, KeyModifiers::NONE), None);
        assert!(a.notice.as_ref().unwrap().contains("Download in progress"));
    }

    #[test]
    fn activated_user_saves_config() {
        let mut a = app();
        assert!(on_notice(
            &mut a,
            Notice::Activated {
                index: 2,
                user: true
            }
        ));
        assert_eq!(a.active, Some(2));
        assert_eq!(a.profile_id, "mx-brown");
        assert!(a.notice.as_ref().unwrap().contains("Loaded"));
    }

    #[test]
    fn activated_startup_does_not_save() {
        let mut a = app();
        assert!(!on_notice(
            &mut a,
            Notice::Activated {
                index: 0,
                user: false
            }
        ));
        assert_eq!(a.active, Some(0));
        assert_eq!(a.profile_id, "mx-blue");
    }

    #[test]
    fn warn_preserves_active_and_preference() {
        let mut a = app();
        a.active = Some(1);
        a.profile_id = "mx-red".into();
        assert!(!on_notice(&mut a, Notice::Warn("boom".into())));
        assert_eq!(a.active, Some(1));
        assert_eq!(a.profile_id, "mx-red");
        assert!(a.notice.as_ref().unwrap().contains("boom"));
    }

    #[test]
    fn volume_steps_and_clamps() {
        let mut a = app();
        assert_eq!(a.volume, 60);
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

    #[test]
    fn markers_show_cursor_and_active() {
        let mut a = app();
        a.active = Some(1);
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("▸"), "{text}");
        assert!(text.contains("♪"), "{text}");
    }

    #[test]
    fn renders_optional_download_marker() {
        let mut a = app();
        a.profiles
            .push(entry("mx-black", "Cherry MX Black (PBT)", true, false));
        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Cherry MX Black (PBT)  ↓"), "{text}");
    }

    #[test]
    fn download_notices_update_entry() {
        let mut a = app();
        a.profiles
            .push(entry("mx-black", "Cherry MX Black (PBT)", true, false));

        assert!(!on_notice(&mut a, Notice::Downloading("mx-black".into())));
        assert!(a.profiles.last().unwrap().downloading);
        assert!(a.notice.as_ref().unwrap().contains("Downloading"));

        assert!(!on_notice(&mut a, Notice::Downloaded("mx-black".into())));
        let entry = a.profiles.last().unwrap();
        assert!(entry.available);
        assert!(!entry.downloading);
        assert!(a.notice.as_ref().unwrap().contains("Downloaded"));
    }

    #[test]
    fn download_failure_clears_busy_state() {
        let mut a = app();
        a.profiles.push(ProfileEntry {
            id: "mx-black".into(),
            name: "Cherry MX Black (PBT)".into(),
            optional: true,
            available: false,
            downloading: true,
        });
        assert!(!on_notice(
            &mut a,
            Notice::DownloadFailed {
                id: "mx-black".into(),
                error: "offline".into(),
            },
        ));
        assert!(!a.profiles.last().unwrap().downloading);
        assert!(a.notice.as_ref().unwrap().contains("offline"));
    }

    #[test]
    fn scrolls_to_selected_optional() {
        let entries: Vec<ProfileEntry> = (0..18)
            .map(|i| entry(&format!("p{i}"), &format!("Pack {i}"), i >= 4, i < 4))
            .collect();
        let mut a = new(entries, &Config::default());
        a.cursor = 17;
        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Pack 17"), "{text}");
    }

    #[test]
    fn renders_all_default_profiles() {
        let (builtins, warnings) = crate::profiles::builtins();
        assert!(warnings.is_empty(), "{warnings:?}");
        let entries: Vec<ProfileEntry> = builtins
            .iter()
            .map(|p| entry(&p.id, &p.name, p.is_optional(), p.is_available(None)))
            .collect();
        let a = new(entries, &Config::default());
        let backend = TestBackend::new(60, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &a)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Cherry MX Blue (ABS)"));
        assert!(text.contains("Cherry MX Red (ABS)"));
        assert!(text.contains("Cherry MX Brown (ABS)"));
        assert!(text.contains("Topre"));
    }
}
