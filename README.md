# discord_rich

Custom Discord Rich Presence for Windows and Linux. Sits in your system tray, watches your active window, and updates Discord based on rules you define. No editor extension needed — works with any app whose window title is informative (VSCode, Cursor, Warp, Windows Terminal, browsers, SSH sessions, etc.).

## Features

- **Universal detection** — uses active window + process name, not editor-specific plugins
- **Rule-based** — declarative TOML rules with regex captures and templating
- **Tray-first** — tray icon with Pause/Resume/Reload/Quit/Settings
- **Native settings UI** — edit rules without touching TOML if you don't want to
- **Cross-platform** — Windows + Linux from a single codebase
- **Tiny binary** — release build is a single small EXE, no runtime needed

## Quick start

### 1. Create a Discord application

1. Go to https://discord.com/developers/applications
2. Click **New Application**, give it a name (this becomes your "playing" name in Discord)
3. Copy the **Application ID** (under "General Information")
4. Optional: under **Rich Presence → Art Assets**, upload images and name them (e.g. `vscode`, `warp`, `ssh`). The names are what you put in `large_image` / `small_image` in your rules.

### 2. Build and run

```powershell
# Debug run (faster compile, shows logs)
cargo run

# Release build (small, optimized EXE)
cargo build --release
# Output: target/release/discord_rich.exe
```

First run creates a default config and writes its location to the log. Open the tray menu → **Settings**, paste your Application ID, click **Save**.

### 3. Customize rules

Either edit rules from the Settings window, or open the config file directly:

- Windows: `%APPDATA%\discord_rich\discord_rich\config.toml`
- Linux:   `~/.config/discord_rich/config.toml`

## Config reference

Each rule has:

| Field          | Type    | Description                                                                 |
|----------------|---------|-----------------------------------------------------------------------------|
| `name`         | string  | Friendly name shown in logs/UI                                              |
| `process`      | string? | Case-insensitive substring match on process name (e.g. `code`, `warp`)      |
| `title_regex`  | string? | Regex matched against the window title; named captures become template vars |
| `priority`     | int     | Higher wins when multiple rules match                                       |
| `details`      | string  | First Discord line; supports `{process}`, `{title}`, and named captures     |
| `state`        | string  | Second Discord line                                                         |
| `large_image`  | string  | Asset key uploaded in the Discord Developer Portal                          |
| `large_text`   | string  | Tooltip shown when hovering the large image                                 |
| `small_image`  | string  | Optional small overlay image                                                |
| `small_text`   | string  | Tooltip for the small image                                                 |
| `show_timer`   | bool    | Whether to show the elapsed-time timer                                      |

A rule must have at least `process` or `title_regex` set.

### Example: SSH detection

```toml
[[rules]]
name = "SSH session"
title_regex = '(?i)ssh\s+(?P<host>[\w\.\-@]+)'
priority = 12
details = "SSH session"
state = "→ {host}"
large_image = "ssh"
large_text = "SSH"
show_timer = true
```

When your terminal title contains `ssh root@example.com`, Discord shows:

```
SSH session
→ root@example.com
```

### Example: VSCode

```toml
[[rules]]
name = "VSCode"
process = "code"
title_regex = '^(?P<file>.+?) - (?P<workspace>.+?) - Visual Studio Code$'
priority = 10
details = "Editing {file}"
state = "in {workspace}"
large_image = "vscode"
large_text = "Visual Studio Code"
show_timer = true
```

## Tray menu

- **Settings…** — opens the native settings window
- **Pause / Resume** — stops or resumes presence updates
- **Reload config** — re-reads `config.toml` from disk
- **Quit** — exits the app

## How detection works

On every poll (default: 5s) the worker:

1. Asks the OS for the foreground window (process name + window title)
2. Walks all rules and picks the highest-priority match
3. Renders the rule's templates with `{process}`, `{title}`, and any named regex captures
4. Pushes the resulting Activity to Discord (via local IPC, no network calls from this app)

If nothing matches and `[idle]` is enabled, the configured idle presence is shown instead.

## Troubleshooting

- **"Discord client_id is empty"** — you haven't pasted your Application ID yet
- **No presence shows up in Discord** — Discord must be running, and the app must be allowed to show your activity (User Settings → Activity Privacy)
- **Wrong app being detected** — check the rule priorities; broader rules should have lower priority than specific ones
- **Window title doesn't match expected format** — open the active window in your terminal and check what the OS reports; some apps include version numbers or extra dashes

## Building a distributable EXE

```powershell
cargo build --release
```

The resulting binary is statically linked (no runtime DLLs needed beyond standard system libs). On Windows, the release build hides the console automatically — log to a file if you want to inspect output:

```powershell
$env:RUST_LOG="debug"; .\target\release\discord_rich.exe *>&1 | Tee-Object -FilePath discord_rich.log
```

## License

MIT
