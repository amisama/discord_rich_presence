use crate::settings_ui;
use crate::worker::WorkerState;
use anyhow::{Context, Result};
use std::sync::Arc;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

/// Build, install, and run the tray icon. This call blocks until the user
/// chooses "Quit" from the menu.
pub fn run(state: Arc<WorkerState>) -> Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    // Build menu items. We keep handles around so we can match MenuEvent IDs.
    let item_settings = MenuItem::new("Settings…", true, None);
    let item_pause = MenuItem::new("Pause", true, None);
    let item_resume = MenuItem::new("Resume", true, None);
    let item_reload = MenuItem::new("Reload config", true, None);
    let item_quit = MenuItem::new("Quit", true, None);

    let menu = Menu::new();
    menu.append(&item_settings)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_pause)?;
    menu.append(&item_resume)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_reload)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&item_quit)?;

    // Sync initial pause state to menu visibility.
    item_pause.set_enabled(!state.is_paused());
    item_resume.set_enabled(state.is_paused());

    let icon = build_default_icon().context("building tray icon")?;
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("discord_rich — Custom Rich Presence")
        .with_icon(icon)
        .build()
        .context("creating tray icon")?;

    let menu_channel = MenuEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    let id_settings = item_settings.id().clone();
    let id_pause = item_pause.id().clone();
    let id_resume = item_resume.id().clone();
    let id_reload = item_reload.id().clone();
    let id_quit = item_quit.id().clone();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        while let Ok(menu_event) = menu_channel.try_recv() {
            let id = menu_event.id();
            if id == &id_settings {
                settings_ui::open(state.clone());
            } else if id == &id_pause {
                state.set_paused(true);
                item_pause.set_enabled(false);
                item_resume.set_enabled(true);
                log::info!("Worker paused");
            } else if id == &id_resume {
                state.set_paused(false);
                item_pause.set_enabled(true);
                item_resume.set_enabled(false);
                log::info!("Worker resumed");
            } else if id == &id_reload {
                match crate::config::Config::load_or_create() {
                    Ok(cfg) => {
                        state.replace_config(cfg);
                        log::info!("Config reloaded from disk");
                    }
                    Err(e) => log::warn!("reload failed: {e:#}"),
                }
            } else if id == &id_quit {
                state.request_shutdown();
                *control_flow = ControlFlow::Exit;
            }
        }

        // Drain tray events to keep the channel from filling up.
        while let Ok(_event) = tray_channel.try_recv() {}
    });
}

/// Custom user event placeholder for the tao event loop. We don't actually
/// publish any of these yet, but the type is required for `with_user_event`.
enum UserEvent {}

/// Generate a 32×32 RGBA icon at startup. We avoid bundling a PNG so the
/// project compiles cleanly without any binary assets in the repo.
fn build_default_icon() -> Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let cx = (SIZE as f32 - 1.0) / 2.0;
    let cy = cx;
    let outer = (SIZE as f32) * 0.5;
    let inner = (SIZE as f32) * 0.18;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            let (r, g, b, a) = if dist > outer {
                (0u8, 0, 0, 0)
            } else if dist < inner {
                (255u8, 255, 255, 255)
            } else {
                // Discord blurple-ish gradient
                let t = (outer - dist) / (outer - inner);
                let r = (88.0 + (255.0 - 88.0) * (1.0 - t)) as u8;
                let g = (101.0 + (255.0 - 101.0) * (1.0 - t)) as u8;
                let b = (242.0 + (255.0 - 242.0) * (1.0 - t)) as u8;
                (r, g, b, 255)
            };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).context("creating Icon from RGBA buffer")
}
