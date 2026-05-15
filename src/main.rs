#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod detector;
mod matcher;
mod presence;
mod settings_ui;
mod tray;
mod worker;

use anyhow::Result;
use std::sync::atomic::Ordering;

fn main() -> Result<()> {
    init_logging();

    let cfg = match config::Config::load_or_create() {
        Ok(c) => c,
        Err(e) => {
            log::error!("failed to load config: {e:#}");
            return Err(e);
        }
    };

    if cfg.general.client_id.trim().is_empty() {
        log::warn!(
            "Discord client_id is empty. Open the tray menu → Settings, paste your \
             Application ID, then Save."
        );
    } else {
        log::info!(
            "Loaded config with {} rule(s); polling every {}s",
            cfg.rules.len(),
            cfg.general.poll_interval_secs
        );
    }

    let state = worker::WorkerState::new(cfg);
    let worker_handle = worker::spawn(state.clone());

    let result = tray::run(state.clone());

    state.request_shutdown();
    state.reload_requested.store(false, Ordering::Relaxed);

    if let Err(e) = worker_handle.join() {
        log::warn!("worker thread join failed: {e:?}");
    }

    result
}

fn init_logging() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::Builder::from_env(env)
        .format_timestamp_secs()
        .init();
}
