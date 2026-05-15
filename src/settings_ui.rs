use crate::config::Config;
use crate::worker::WorkerState;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Global flag so we never spawn more than one settings window at a time.
static SETTINGS_OPEN: AtomicBool = AtomicBool::new(false);

/// Open the settings window in a dedicated thread. Returns immediately.
pub fn open(state: Arc<WorkerState>) {
    if SETTINGS_OPEN
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::info!("Settings window already open, ignoring request");
        return;
    }

    thread::Builder::new()
        .name("discord_rich-settings".into())
        .spawn(move || {
            let initial = state.config.lock().clone();
            let app = SettingsApp::new(state.clone(), initial);

            let mut options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([520.0, 600.0])
                    .with_min_inner_size([460.0, 480.0])
                    .with_title("discord_rich settings"),
                ..Default::default()
            };

            // winit normally refuses to start an event loop off the main
            // thread on Windows and Wayland. We're explicitly opting in here
            // because we run egui in a dedicated worker thread so the tray
            // event loop stays responsive on the main thread.
            options.event_loop_builder = Some(Box::new(|builder| {
                #[cfg(target_os = "windows")]
                {
                    use winit::platform::windows::EventLoopBuilderExtWindows;
                    builder.with_any_thread(true);
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    use winit::platform::wayland::EventLoopBuilderExtWayland;
                    use winit::platform::x11::EventLoopBuilderExtX11;
                    EventLoopBuilderExtWayland::with_any_thread(builder, true);
                    EventLoopBuilderExtX11::with_any_thread(builder, true);
                }
                let _ = builder;
            }));

            if let Err(e) = eframe::run_native(
                "discord_rich settings",
                options,
                Box::new(|_cc| Ok(Box::new(app))),
            ) {
                log::error!("settings window crashed: {e}");
            }

            SETTINGS_OPEN.store(false, Ordering::SeqCst);
        })
        .expect("failed to spawn settings thread");
}

struct SettingsApp {
    state: Arc<WorkerState>,
    draft: Config,
    save_message: Option<String>,
}

impl SettingsApp {
    fn new(state: Arc<WorkerState>, draft: Config) -> Self {
        Self {
            state,
            draft,
            save_message: None,
        }
    }

    fn save(&mut self) {
        match self.draft.save() {
            Ok(()) => {
                self.state.replace_config(self.draft.clone());
                self.save_message = Some("Saved. Worker reloaded.".into());
                log::info!("Settings saved from UI");
            }
            Err(e) => {
                self.save_message = Some(format!("Save failed: {e:#}"));
                log::warn!("Settings save failed: {e:#}");
            }
        }
    }

    fn reload_from_disk(&mut self) {
        match Config::load_or_create() {
            Ok(cfg) => {
                self.draft = cfg.clone();
                self.state.replace_config(cfg);
                self.save_message = Some("Reloaded from disk.".into());
            }
            Err(e) => {
                self.save_message = Some(format!("Reload failed: {e:#}"));
            }
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("discord_rich");
                ui.separator();
                let path = Config::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "<unknown>".into());
                ui.label(format!("config: {path}"));
            });
        });

        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Reload from disk").clicked() {
                    self.reload_from_disk();
                }
                if ui.button("Open config folder").clicked() {
                    if let Ok(dir) = Config::config_dir() {
                        let _ = open_path(&dir.display().to_string());
                    }
                }
                if let Some(msg) = &self.save_message {
                    ui.separator();
                    ui.label(msg);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("General");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Discord Client ID");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.general.client_id)
                            .desired_width(280.0)
                            .hint_text("e.g. 1234567890123456789"),
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "Get one at https://discord.com/developers/applications",
                    )
                    .small()
                    .weak(),
                );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Poll interval (seconds)");
                    let mut value = self.draft.general.poll_interval_secs as i32;
                    if ui
                        .add(egui::DragValue::new(&mut value).range(1..=60))
                        .changed()
                    {
                        self.draft.general.poll_interval_secs = value.max(1) as u64;
                    }
                });
                ui.checkbox(
                    &mut self.draft.general.reset_timer_on_switch,
                    "Reset elapsed timer when active app changes",
                );
                ui.checkbox(
                    &mut self.draft.general.start_minimized,
                    "Start minimized to tray",
                );

                ui.add_space(12.0);
                ui.heading("Idle presence");
                ui.label(
                    egui::RichText::new(
                        "Shown when no rule matches the active window.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(4.0);
                ui.checkbox(&mut self.draft.idle.enabled, "Show idle presence");
                ui.add_enabled_ui(self.draft.idle.enabled, |ui| {
                    grid_two("idle_grid", ui, |ui| {
                        ui.label("Details");
                        ui.text_edit_singleline(&mut self.draft.idle.details);
                        ui.end_row();

                        ui.label("State");
                        ui.text_edit_singleline(&mut self.draft.idle.state);
                        ui.end_row();

                        ui.label("Large image key");
                        ui.text_edit_singleline(&mut self.draft.idle.large_image);
                        ui.end_row();

                        ui.label("Large text");
                        ui.text_edit_singleline(&mut self.draft.idle.large_text);
                        ui.end_row();
                    });
                });

                ui.add_space(12.0);
                ui.heading("Rules");
                ui.label(
                    egui::RichText::new(
                        "Higher priority wins when multiple rules match. Use \
                         {process}, {title}, and any named regex captures \
                         (e.g. {file}, {host}) inside details/state.",
                    )
                    .small()
                    .weak(),
                );

                let mut to_remove: Option<usize> = None;
                for (idx, rule) in self.draft.rules.iter_mut().enumerate() {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new(format!(
                        "{}  (priority {})",
                        if rule.name.is_empty() {
                            "(unnamed)"
                        } else {
                            rule.name.as_str()
                        },
                        rule.priority
                    ))
                    .id_salt(format!("rule_{idx}"))
                    .default_open(idx < 2)
                    .show(ui, |ui| {
                        grid_two(&format!("rule_grid_{idx}"), ui, |ui| {
                            ui.label("Name");
                            ui.text_edit_singleline(&mut rule.name);
                            ui.end_row();

                            ui.label("Process contains");
                            let mut process = rule.process.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut process).changed() {
                                rule.process = if process.trim().is_empty() {
                                    None
                                } else {
                                    Some(process)
                                };
                            }
                            ui.end_row();

                            ui.label("Title regex");
                            let mut regex = rule.title_regex.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut regex).changed() {
                                rule.title_regex = if regex.trim().is_empty() {
                                    None
                                } else {
                                    Some(regex)
                                };
                            }
                            ui.end_row();

                            ui.label("Priority");
                            ui.add(egui::DragValue::new(&mut rule.priority).range(0..=100));
                            ui.end_row();

                            ui.label("Details");
                            ui.text_edit_singleline(&mut rule.details);
                            ui.end_row();

                            ui.label("State");
                            ui.text_edit_singleline(&mut rule.state);
                            ui.end_row();

                            ui.label("Large image key");
                            ui.text_edit_singleline(&mut rule.large_image);
                            ui.end_row();

                            ui.label("Large text");
                            ui.text_edit_singleline(&mut rule.large_text);
                            ui.end_row();

                            ui.label("Small image key");
                            ui.text_edit_singleline(&mut rule.small_image);
                            ui.end_row();

                            ui.label("Small text");
                            ui.text_edit_singleline(&mut rule.small_text);
                            ui.end_row();
                        });

                        ui.checkbox(&mut rule.show_timer, "Show elapsed timer");

                        ui.horizontal(|ui| {
                            if ui.button("Remove rule").clicked() {
                                to_remove = Some(idx);
                            }
                        });
                    });
                }
                if let Some(idx) = to_remove {
                    self.draft.rules.remove(idx);
                }

                ui.add_space(8.0);
                if ui.button("Add new rule").clicked() {
                    self.draft.rules.push(crate::config::Rule {
                        name: "New rule".into(),
                        process: None,
                        title_regex: None,
                        priority: 1,
                        details: "{title}".into(),
                        state: String::new(),
                        large_image: String::new(),
                        large_text: String::new(),
                        small_image: String::new(),
                        small_text: String::new(),
                        show_timer: true,
                    });
                }
            });
        });
    }
}

fn grid_two<R>(
    id: &str,
    ui: &mut egui::Ui,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .striped(true)
        .show(ui, body)
        .inner
}

fn open_path(path: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    Ok(())
}
