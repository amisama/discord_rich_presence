use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Top-level configuration loaded from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: General,
    #[serde(default)]
    pub idle: Idle,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    /// Discord Application (Client) ID. Get one from https://discord.com/developers/applications
    pub client_id: String,
    /// How often to poll the active window, in seconds.
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// Reset the elapsed timer every time the active app changes.
    #[serde(default = "default_true")]
    pub reset_timer_on_switch: bool,
    /// Start automatically without showing the settings window.
    #[serde(default = "default_true")]
    pub start_minimized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idle {
    /// Show this presence when no rule matches.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_idle_details")]
    pub details: String,
    #[serde(default = "default_idle_state")]
    pub state: String,
    #[serde(default = "default_idle_image")]
    pub large_image: String,
    #[serde(default)]
    pub large_text: String,
}

/// A rule maps a detected window/process to a Discord presence payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Friendly name for this rule (shown in logs/UI).
    pub name: String,
    /// Match against the process name (case-insensitive substring). Optional.
    #[serde(default)]
    pub process: Option<String>,
    /// Match against the window title (regex). Optional.
    #[serde(default)]
    pub title_regex: Option<String>,
    /// Higher priority wins when multiple rules match. Default: 0.
    #[serde(default)]
    pub priority: i32,
    /// First line of the presence (max ~128 chars on Discord).
    pub details: String,
    /// Second line of the presence.
    #[serde(default)]
    pub state: String,
    /// Asset key (must be uploaded under "Rich Presence > Art Assets" in Discord Developer Portal).
    #[serde(default)]
    pub large_image: String,
    /// Tooltip when hovering the large image.
    #[serde(default)]
    pub large_text: String,
    #[serde(default)]
    pub small_image: String,
    #[serde(default)]
    pub small_text: String,
    /// Whether to show the elapsed timer for this rule.
    #[serde(default = "default_true")]
    pub show_timer: bool,
}

impl Default for Idle {
    fn default() -> Self {
        Self {
            enabled: true,
            details: default_idle_details(),
            state: default_idle_state(),
            large_image: default_idle_image(),
            large_text: String::new(),
        }
    }
}

fn default_poll_interval() -> u64 {
    5
}
fn default_true() -> bool {
    true
}
fn default_idle_details() -> String {
    "Idle".to_string()
}
fn default_idle_state() -> String {
    "Doing nothing".to_string()
}
fn default_idle_image() -> String {
    "default".to_string()
}

impl Config {
    /// Returns the canonical config directory for this app.
    pub fn config_dir() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("dev", "discord_rich", "discord_rich")
            .context("could not resolve a project directory")?;
        Ok(dirs.config_dir().to_path_buf())
    }

    /// Path to the active config file.
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Load the config, creating a default one on first run.
    pub fn load_or_create() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            let parent = path.parent().context("config path has no parent")?;
            fs::create_dir_all(parent).context("failed to create config dir")?;
            let default = Self::default();
            default.save_to(&path)?;
            log::info!("Created default config at {}", path.display());
            return Ok(default);
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        let cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing config at {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let raw = toml::to_string_pretty(self).context("serializing config")?;
        fs::write(path, raw).with_context(|| format!("writing config to {}", path.display()))?;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General {
                client_id: String::new(),
                poll_interval_secs: default_poll_interval(),
                reset_timer_on_switch: true,
                start_minimized: true,
            },
            idle: Idle::default(),
            rules: default_rules(),
        }
    }
}

fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "VSCode".into(),
            process: Some("code".into()),
            title_regex: Some(r"^(?P<file>.+?) - (?P<workspace>.+?) - Visual Studio Code$".into()),
            priority: 10,
            details: "Editing {file}".into(),
            state: "in {workspace}".into(),
            large_image: "vscode".into(),
            large_text: "Visual Studio Code".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        },
        Rule {
            name: "Cursor".into(),
            process: Some("cursor".into()),
            title_regex: Some(r"^(?P<file>.+?) - (?P<workspace>.+?) - Cursor$".into()),
            priority: 10,
            details: "Editing {file}".into(),
            state: "in {workspace}".into(),
            large_image: "cursor".into(),
            large_text: "Cursor".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        },
        Rule {
            name: "Warp Terminal".into(),
            process: Some("warp".into()),
            title_regex: None,
            priority: 8,
            details: "In a terminal".into(),
            state: "{title}".into(),
            large_image: "warp".into(),
            large_text: "Warp".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        },
        Rule {
            name: "Windows Terminal".into(),
            process: Some("windowsterminal".into()),
            title_regex: None,
            priority: 8,
            details: "In a terminal".into(),
            state: "{title}".into(),
            large_image: "terminal".into(),
            large_text: "Windows Terminal".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        },
        Rule {
            name: "SSH session".into(),
            process: None,
            title_regex: Some(r"(?i)ssh\s+(?P<host>[\w\.\-@]+)".into()),
            priority: 12,
            details: "SSH session".into(),
            state: "→ {host}".into(),
            large_image: "ssh".into(),
            large_text: "SSH".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: true,
        },
        Rule {
            name: "Browser".into(),
            process: Some("chrome".into()),
            title_regex: None,
            priority: 3,
            details: "Browsing".into(),
            state: "{title}".into(),
            large_image: "chrome".into(),
            large_text: "Google Chrome".into(),
            small_image: String::new(),
            small_text: String::new(),
            show_timer: false,
        },
    ]
}
