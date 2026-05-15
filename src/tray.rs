use crate::settings_ui;
use crate::worker::WorkerState;
use anyhow::{Context, Result};
use std::sync::Arc;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

/// Custom user event that wakes the tao event loop when a menu or tray
/// click happens. Without this, menu clicks land on a side channel and the
/// loop never reacts because it's parked on `ControlFlow::Wait`.
enum UserEvent {
    Menu(MenuEvent),
    /// Tray icon clicks (left/right on the icon itself). We forward them so
    /// the loop wakes, but we don't act on them — the popup menu handles
    /// right-click for us.
    #[allow(dead_code)]
    Tray(TrayIconEvent),
}

/// Build, install, and run the tray icon. This call blocks until the user
/// chooses "Quit" from the menu.
pub fn run(state: Arc<WorkerState>) -> Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    // Build menu items first so we can capture their IDs.
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
    // Tray icon must outlive the event loop. We move it into the closure so
    // it stays alive for the entire app lifetime.
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("discord_rich — Custom Rich Presence")
        .with_icon(icon)
        .build()
        .context("creating tray icon")?;

    // Forward menu and tray events into the tao event loop. This is the
    // critical wiring: without these handlers, clicks would queue on the
    // global channels but our `ControlFlow::Wait` loop would never wake up.
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let tray_proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = tray_proxy.send_event(UserEvent::Tray(event));
    }));

    let id_settings: MenuId = item_settings.id().clone();
    let id_pause: MenuId = item_pause.id().clone();
    let id_resume: MenuId = item_resume.id().clone();
    let id_reload: MenuId = item_reload.id().clone();
    let id_quit: MenuId = item_quit.id().clone();

    event_loop.run(move |event, _, control_flow| {
        // Sleep until the next user-initiated event.
        *control_flow = ControlFlow::Wait;

        // Hold the tray icon alive for the loop's whole lifetime.
        let _keep_tray_alive = &tray;

        if let Event::UserEvent(UserEvent::Menu(menu_event)) = event {
            let id = menu_event.id();
            if id == &id_settings {
                log::debug!("tray: Settings clicked");
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
                log::info!("tray: Quit requested");
                state.request_shutdown();
                *control_flow = ControlFlow::Exit;
            }
        }
        // Tray-icon-only events (left/right click on the icon itself) are
        // ignored: the menu attached to the icon already handles right-click.
    });
}

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
