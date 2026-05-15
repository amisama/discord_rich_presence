use crate::config::Config;
use crate::detector;
use crate::matcher::Matcher;
use crate::presence::PresenceClient;
use anyhow::Result;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Shared state controlled by the tray menu and read by the worker.
pub struct WorkerState {
    pub config: Mutex<Config>,
    pub paused: AtomicBool,
    pub reload_requested: AtomicBool,
    pub shutdown: AtomicBool,
}

impl WorkerState {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config: Mutex::new(config),
            paused: AtomicBool::new(false),
            reload_requested: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        })
    }

    pub fn replace_config(&self, new_cfg: Config) {
        *self.config.lock() = new_cfg;
        self.reload_requested.store(true, Ordering::Relaxed);
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

/// Spawn the background worker thread that drives the presence loop.
pub fn spawn(state: Arc<WorkerState>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("discord_rich-worker".into())
        .spawn(move || {
            if let Err(e) = run(state) {
                log::error!("worker thread terminated with error: {e:#}");
            }
        })
        .expect("failed to spawn worker thread")
}

fn run(state: Arc<WorkerState>) -> Result<()> {
    let mut snapshot = state.config.lock().clone();
    let mut matcher = Matcher::from_config(&snapshot);
    let mut presence: Option<PresenceClient> = None;
    let mut last_was_paused = false;

    loop {
        if state.shutdown.load(Ordering::Relaxed) {
            if let Some(mut p) = presence.take() {
                p.shutdown();
            }
            return Ok(());
        }

        if state.reload_requested.swap(false, Ordering::Relaxed) {
            snapshot = state.config.lock().clone();
            matcher = Matcher::from_config(&snapshot);
            // Reconnect the IPC client so a new client_id takes effect.
            if let Some(mut p) = presence.take() {
                p.shutdown();
            }
            log::info!("Worker reloaded config");
        }

        let poll_secs = snapshot.general.poll_interval_secs.max(1);

        if state.is_paused() {
            if !last_was_paused {
                if let Some(p) = presence.as_mut() {
                    if let Err(e) = p.clear() {
                        log::warn!("failed to clear presence on pause: {e:#}");
                    }
                }
                last_was_paused = true;
            }
            sleep_with_shutdown(&state, Duration::from_secs(poll_secs));
            continue;
        }
        last_was_paused = false;

        // Lazily construct the presence client once we have a valid client_id.
        if presence.is_none() {
            let client_id = snapshot.general.client_id.trim();
            if client_id.is_empty() {
                log::warn!(
                    "Discord client_id is empty. Set it in {}",
                    crate::config::Config::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "the config file".into())
                );
                sleep_with_shutdown(&state, Duration::from_secs(poll_secs.max(10)));
                continue;
            }
            match PresenceClient::new(client_id) {
                Ok(p) => presence = Some(p),
                Err(e) => {
                    log::warn!("failed to init Discord client: {e:#}");
                    sleep_with_shutdown(&state, Duration::from_secs(poll_secs.max(10)));
                    continue;
                }
            }
        }

        match detector::current_active_app() {
            Ok(Some(app)) => {
                let resolved = matcher
                    .match_app(&app)
                    .or_else(|| Matcher::idle_presence(&snapshot));

                let p = presence.as_mut().expect("presence client just initialized");
                let result = match resolved {
                    Some(r) => p.apply(&r, snapshot.general.reset_timer_on_switch),
                    None => p.clear(),
                };
                if let Err(e) = result {
                    log::warn!("presence update failed (will retry): {e:#}");
                    presence = None; // force reconnect on next iteration
                }
            }
            Ok(None) => {
                if let Some(idle) = Matcher::idle_presence(&snapshot) {
                    if let Some(p) = presence.as_mut() {
                        if let Err(e) = p.apply(&idle, snapshot.general.reset_timer_on_switch) {
                            log::warn!("idle presence update failed: {e:#}");
                            presence = None;
                        }
                    }
                } else if let Some(p) = presence.as_mut() {
                    let _ = p.clear();
                }
            }
            Err(e) => {
                log::debug!("active window probe failed: {e:#}");
            }
        }

        sleep_with_shutdown(&state, Duration::from_secs(poll_secs));
    }
}

fn sleep_with_shutdown(state: &WorkerState, total: Duration) {
    let step = Duration::from_millis(250);
    let mut remaining = total;
    while remaining > Duration::ZERO {
        if state.shutdown.load(Ordering::Relaxed) {
            return;
        }
        if state.reload_requested.load(Ordering::Relaxed) {
            return;
        }
        let chunk = if remaining < step { remaining } else { step };
        thread::sleep(chunk);
        remaining = remaining.saturating_sub(chunk);
    }
}
