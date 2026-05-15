use crate::matcher::{clamp_for_discord, ResolvedPresence};
use anyhow::{anyhow, Result};
use discord_rich_presence::{
    activity::{Activity, Assets, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wraps a Discord IPC client and tracks the last applied presence so we only
/// push updates when something actually changes.
pub struct PresenceClient {
    client: DiscordIpcClient,
    connected: bool,
    last_applied: Option<ResolvedPresence>,
    last_started_at: Option<i64>,
    last_rule: Option<String>,
}

impl PresenceClient {
    pub fn new(client_id: &str) -> Result<Self> {
        // discord-rich-presence returns Box<dyn Error>, which doesn't satisfy
        // anyhow::Context bounds, so map manually.
        let client = DiscordIpcClient::new(client_id)
            .map_err(|e| anyhow!("failed to construct Discord IPC client: {e}"))?;
        Ok(Self {
            client,
            connected: false,
            last_applied: None,
            last_started_at: None,
            last_rule: None,
        })
    }

    pub fn ensure_connected(&mut self) -> Result<()> {
        if self.connected {
            return Ok(());
        }
        self.client
            .connect()
            .map_err(|e| anyhow!("failed to connect to Discord IPC (is Discord running?): {e}"))?;
        self.connected = true;
        log::info!("Connected to Discord IPC");
        Ok(())
    }

    /// Push the resolved presence to Discord. No-op if nothing changed.
    pub fn apply(&mut self, presence: &ResolvedPresence, reset_timer_on_switch: bool) -> Result<()> {
        self.ensure_connected()?;

        // Skip duplicate updates. Discord rate-limits to ~5/20s.
        if self
            .last_applied
            .as_ref()
            .map(|prev| prev == presence)
            .unwrap_or(false)
        {
            return Ok(());
        }

        let switched_rule = self
            .last_rule
            .as_deref()
            .map(|r| r != presence.rule_name)
            .unwrap_or(true);

        let started_at = if reset_timer_on_switch && switched_rule {
            now_unix_seconds()
        } else {
            self.last_started_at.unwrap_or_else(now_unix_seconds)
        };

        let details = clamp_for_discord(&presence.details);
        let state = clamp_for_discord(&presence.state);
        let large_text = clamp_for_discord(&presence.large_text);
        let small_text = clamp_for_discord(&presence.small_text);

        let mut activity = Activity::new();
        if !details.is_empty() {
            activity = activity.details(&details);
        }
        if !state.is_empty() {
            activity = activity.state(&state);
        }

        let mut assets = Assets::new();
        let mut has_assets = false;
        if !presence.large_image.is_empty() {
            assets = assets.large_image(&presence.large_image);
            has_assets = true;
            if !large_text.is_empty() {
                assets = assets.large_text(&large_text);
            }
        }
        if !presence.small_image.is_empty() {
            assets = assets.small_image(&presence.small_image);
            has_assets = true;
            if !small_text.is_empty() {
                assets = assets.small_text(&small_text);
            }
        }
        if has_assets {
            activity = activity.assets(assets);
        }

        if presence.show_timer {
            activity = activity.timestamps(Timestamps::new().start(started_at));
        }

        if let Err(e) = self.client.set_activity(activity) {
            // Connection might have dropped. Mark disconnected so the next call reconnects.
            self.connected = false;
            return Err(anyhow::anyhow!("failed to update presence: {e}"));
        }

        self.last_applied = Some(presence.clone());
        self.last_started_at = Some(started_at);
        self.last_rule = Some(presence.rule_name.clone());
        log::debug!(
            "Presence updated → rule='{}' details='{}' state='{}'",
            presence.rule_name,
            details,
            state
        );
        Ok(())
    }

    /// Clear the active presence (e.g. when paused or going idle).
    pub fn clear(&mut self) -> Result<()> {
        if !self.connected {
            return Ok(());
        }
        if let Err(e) = self.client.clear_activity() {
            self.connected = false;
            return Err(anyhow::anyhow!("failed to clear presence: {e}"));
        }
        self.last_applied = None;
        self.last_started_at = None;
        self.last_rule = None;
        log::debug!("Presence cleared");
        Ok(())
    }

    pub fn shutdown(&mut self) {
        if self.connected {
            let _ = self.client.close();
            self.connected = false;
        }
    }
}

impl Drop for PresenceClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
