# Records assets/demo.gif for the README.
# Run from the repository root: cargo build --release && vhs assets/demo.tape
Output assets/demo.gif

Set Shell "bash"
Set FontSize 16
Set Width 960
Set Height 700
Set Padding 20
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

# Start the TUI on a throwaway HOME so the recording always begins from the
# default config (mx-blue, 60% volume) instead of the developer's saved state.
# The prompt and the command itself are not recorded.
Hide
Type 'export HOME="$PWD/target/demo-home"; rm -rf "$HOME"; mkdir -p "$HOME"; ./target/release/brokey'
Enter
Sleep 1500ms
Show

Sleep 800ms
# Browse down to an embedded profile and load it.
Down
Down
Down
Sleep 400ms
Enter
Sleep 900ms
# Raise the volume.
Right
Right
Sleep 600ms
# Pause, then resume listening.
Space
Sleep 700ms
Space
Sleep 500ms
# Replay the active profile.
Type "p"
Sleep 800ms
