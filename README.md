# brokey

Real thock. Fake keyboard. Zero budget.

Brokey plays mechanical-switch sounds on every global keypress, with a small terminal UI.

## Install / run

```bash
cargo install --path .
cargo run --release
```

## Build dependencies (Linux)

On Debian/Ubuntu:

```bash
sudo apt install libasound2-dev libx11-dev libxtst-dev
```

## Permissions

- **macOS:** grant **Input Monitoring** _and_ **Accessibility** to the terminal app in System Settings → Privacy & Security, then restart brokey.
- **Linux:** needs an X11 session; on Wayland only XWayland apps are heard (documented, not worked around).
- **Windows:** none.

## Keys

| Key                  | Action                                      |
| -------------------- | ------------------------------------------- |
| `↑` / `k`            | Select profile (switching also previews it) |
| `↓` / `j`            | Select profile (switching also previews it) |
| `←` / `-`            | Volume −10% (0–100)                         |
| `→` / `+` / `=`      | Volume +10% (0–100)                         |
| `Space`              | Pause/resume                                |
| `p`                  | Preview the current profile                 |
| `q`, `Esc`, `Ctrl-C` | Quit                                        |

## Config

Config location: `<config dir>/brokey/config.toml`, where the config dir is what `dirs::config_dir()` returns (macOS `~/Library/Application Support`, Linux `~/.config`, Windows `%APPDATA%`).

Example:

```toml
profile = "mx-blue"
volume = 60
```

Volume is an integer 0–100. Preferences save automatically on change.

## Built-in profiles

- `mx-blue` "Cherry MX Blue"
- `mx-red` "Cherry MX Red"
- `mx-brown` "Cherry MX Brown"
- `topre` "Topre"
- `mx-black` "Cherry MX Black"
- `holy-panda` "Holy Panda"
- `nk-cream` "NK Cream"
- `eg-oreo` "Everglide Oreo"
- `eg-crystal-purple` "Everglide Crystal Purple"
- `turquoise` "Turquoise Tealios"
- `mx-black-abs` "Cherry MX Black (ABS)"

Built-in profiles use real per-key recordings: letters, digits, common
punctuation, Space, Enter, Backspace, and Tab each have their own sample where
the source pack provides one. Keys without a recording, and the preview key,
rotate through natural fallback takes from the same pack.

## Custom sound packs

Put custom packs in `<config dir>/brokey/sounds/<name>/`.
`<name>/press.wav` is required; optional `space.wav`. The directory name becomes the profile id and name. A pack needs a file with stem `press` (any extension). Packs that collide with a built-in id or fail to decode are skipped with a warning shown in the UI.

The bundled sounds are real recordings from MIT-licensed packs; see `assets/sounds/CREDITS.md` for provenance and attribution.

Brokey uses the keycode only to choose a sound; it does not build, log, or store
text. No audio is recorded.
