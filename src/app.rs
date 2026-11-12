use crate::devices::{groups, Inventory};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

type Scan = Result<Vec<crate::devices::Device>, String>;
const ACCENT: Color32 = Color32::from_rgb(145, 216, 166);

struct Worker {
    pending: Arc<Mutex<Option<Scan>>>,
    stop: Arc<AtomicBool>,
    wake: SyncSender<()>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(ctx: egui::Context) -> Result<Self, String> {
        let pending = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (wake, receiver) = mpsc::sync_channel(1);
        let output = pending.clone();
        let stopping = stop.clone();
        let handle = thread::Builder::new()
            .name("device-metadata".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    #[cfg(windows)]
                    let scan = crate::windows_devices::enumerate();
                    #[cfg(not(windows))]
                    let scan: Scan =
                        Err("Device enumeration currently supports Windows only.".into());
                    if let Ok(mut slot) = output.lock() {
                        *slot = Some(scan);
                    }
                    ctx.request_repaint();
                    match receiver.recv_timeout(Duration::from_secs(2)) {
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        _ => continue,
                    }
                }
            })
            .map_err(|error| format!("Cannot start device worker: {error}"))?;
        Ok(Self {
            pending,
            stop,
            wake,
            handle: Some(handle),
        })
    }

    fn refresh(&self) {
        let _ = self.wake.try_send(());
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(handle) = self.handle.take() {
            // ponytail: SetupAPI cannot be cancelled; detach an active call rather than freeze exit.
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Preferences {
    light: bool,
    scale: f32,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            light: false,
            scale: 1.0,
        }
    }
}

fn parse_preferences(text: &str) -> Result<Preferences, String> {
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 3 || lines[0] != "device-lab-preferences-v1" {
        return Err("Unrecognized preferences; original file preserved.".into());
    }
    let light = match lines[1] {
        "theme=dark" => false,
        "theme=light" => true,
        _ => return Err("Invalid theme; original preferences preserved.".into()),
    };
    let scale = lines[2]
        .strip_prefix("scale=")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|s| s.is_finite() && (0.8..=1.6).contains(s))
        .ok_or("Invalid scale; original preferences preserved.")?;
    Ok(Preferences { light, scale })
}

fn preference_path() -> PathBuf {
    // Explicit project-local path; never writes relative to the launch directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("preferences.txt")
}

fn save_preferences(prefs: &Preferences) -> Result<(), String> {
    let path = preference_path();
    let parent = path.parent().ok_or("Missing preference parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        use std::io::Write;
        write!(
            file,
            "device-lab-preferences-v1\ntheme={}\nscale={}\n",
            if prefs.light { "light" } else { "dark" },
            prefs.scale
        )?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers valid for this call.
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, &path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("Could not save preferences: {e}"))
}

pub struct DeviceLab {
    inventory: Inventory,
    worker: Option<Worker>,
    search: String,
    class: String,
    selected: Option<String>,
    settings: bool,
    prefs: Preferences,
    prefs_error: Option<String>,
    save_message: Option<String>,
}

impl DeviceLab {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let loaded = match fs::read_to_string(preference_path()) {
            Ok(text) => parse_preferences(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(e) => Err(format!("Cannot read preferences: {e}. Original preserved.")),
        };
        let (prefs, prefs_error) = match loaded {
            Ok(p) => (p, None),
            Err(e) => (Preferences::default(), Some(e)),
        };
        cc.egui_ctx.set_zoom_factor(prefs.scale);
        cc.egui_ctx.set_visuals(if prefs.light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        });
        let mut inventory = Inventory::default();
        let worker = match Worker::new(cc.egui_ctx.clone()) {
            Ok(worker) => Some(worker),
            Err(error) => {
                inventory.apply(Err(error));
                None
            }
        };
        Self {
            inventory,
            worker,
            search: String::new(),
            class: String::new(),
            selected: None,
            settings: false,
            prefs,
            prefs_error,
            save_message: None,
        }
    }

    fn details(&mut self, ctx: &egui::Context) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        egui::SidePanel::right("details")
            .default_width(310.0)
            .min_width(250.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.heading("DEVICE DETAILS");
                    if ui.button("Close").clicked() {
                        self.selected = None;
                    }
                });
                ui.separator();
                let grouped = groups(&self.inventory.devices);
                let Some(group) = grouped.iter().find(|g| g.key == selected) else {
                    ui.label("This device is no longer present in the latest successful scan.");
                    return;
                };
                if self.inventory.error.is_some() {
                    ui.colored_label(Color32::YELLOW, "Stale metadata — current presence unknown");
                }
                ui.label(RichText::new(group.members[0].title()).size(20.0));
                ui.label("Information only · hardware configuration unavailable");
                egui::ScrollArea::vertical()
                    .id_salt("details_scroll")
                    .show(ui, |ui| {
                        for device in &group.members {
                            egui::CollapsingHeader::new(device.title())
                                .id_salt(device.key())
                                .default_open(group.members.len() == 1)
                                .show(ui, |ui| {
                                    for (key, value) in [
                                        ("Class", device.class.as_str()),
                                        ("Manufacturer", device.manufacturer.as_str()),
                                        ("OS enumerator", device.enumerator.as_str()),
                                        ("Instance ID", device.instance_id.as_str()),
                                        (
                                            "Container ID",
                                            device.container_id.as_deref().unwrap_or("Unknown"),
                                        ),
                                        ("Parent", device.parent.as_str()),
                                    ] {
                                        ui.label(RichText::new(key).strong());
                                        ui.add(
                                            egui::Label::new(if value.is_empty() {
                                                "Unknown"
                                            } else {
                                                value
                                            })
                                            .wrap()
                                            .selectable(true),
                                        );
                                        ui.add_space(6.0);
                                    }
                                    if let Some((vid, pid)) = device.usb_ids() {
                                        ui.label(format!("VID {vid:04X} · PID {pid:04X}"));
                                    }
                                    for id in &device.hardware_ids {
                                        ui.add(egui::Label::new(id).wrap().selectable(true));
                                    }
                                });
                        }
                    });
            });
    }

    fn preferences(&mut self, ctx: &egui::Context) {
        egui::Window::new("Appearance").open(&mut self.settings).resizable(false).show(ctx, |ui| {
            if ui.checkbox(&mut self.prefs.light, "Light theme").changed() {
                ctx.set_visuals(if self.prefs.light { egui::Visuals::light() } else { egui::Visuals::dark() });
            }
            if ui.add(egui::Slider::new(&mut self.prefs.scale, 0.8..=1.6).text("UI scale")).changed() {
                ctx.set_zoom_factor(self.prefs.scale);
            }
            if let Some(error) = &self.prefs_error {
                ui.colored_label(Color32::YELLOW, error);
                ui.label("Saving disabled to preserve the original file. Move it aside manually, then restart.");
            }
            if ui.add_enabled(self.prefs_error.is_none(), egui::Button::new("Save locally")).clicked() {
                self.save_message = Some(match save_preferences(&self.prefs) { Ok(()) => "Saved in project data folder.".into(), Err(e) => e });
            }
            if let Some(message) = &self.save_message { ui.label(message); }
            ui.label("No device inventory is saved.");
        });
    }
}

impl eframe::App for DeviceLab {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(worker) = &self.worker {
            let pending = worker.pending.lock().ok().and_then(|mut slot| slot.take());
            if let Some(scan) = pending {
                self.inventory.apply(scan);
            }
        }
        let focus_search = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F));
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.selected.is_some() {
                self.selected = None;
            } else {
                self.search.clear();
            }
        }
        egui::TopBottomPanel::top("navigation").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("DEVICE LAB").size(21.0).strong());
                ui.add_space(30.0);
                if ui.selectable_label(true, "DASHBOARD").clicked() {
                    self.selected = None;
                }
                ui.label("NATIVE RUST / GPU");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Appearance").clicked() {
                        self.settings = true;
                    }
                    if ui.button("Refresh").clicked() {
                        if let Some(worker) = &self.worker {
                            worker.refresh();
                        }
                    }
                });
            });
            ui.add_space(12.0);
        });
        egui::TopBottomPanel::bottom("safety").show(ctx, |ui| {
            ui.label("Read-only OS metadata · Bluetooth presence is not connection status · Unpaired BLE/network discovery excluded");
        });
        egui::SidePanel::left("filters")
            .exact_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(20.0);
                ui.label(RichText::new("DEVICES").strong());
                ui.add_space(12.0);
                ui.selectable_value(&mut self.class, String::new(), "All Windows devices");
                let mut classes: Vec<_> = self
                    .inventory
                    .devices
                    .iter()
                    .map(|d| d.class.clone())
                    .filter(|c| !c.is_empty())
                    .collect();
                classes.sort();
                classes.dedup();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for class in classes {
                        ui.selectable_value(&mut self.class, class.clone(), &class);
                    }
                });
            });
        self.details(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(20.0);
            ui.heading("YOUR DEVICES");
            ui.label("Explore hardware Windows currently exposes. Any vendor. No device commands.");
            ui.add_space(14.0);
            let search = ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search devices, class or identifiers · Ctrl+F").desired_width(420.0));
            if focus_search { search.request_focus(); }
            ui.add_space(10.0);
            if let Some(error) = &self.inventory.error {
                ui.colored_label(Color32::YELLOW, format!("{}: {error}", if self.inventory.last_good.is_some() { "Last scan failed; showing stale data" } else { "Discovery unavailable" }));
            } else if self.inventory.last_good.is_none() {
                ui.label("Discovering devices…");
            } else {
                let changes = &self.inventory.changes;
                ui.label(format!("{} OS records · last scan: +{} added / {} updated / −{} removed",
                    self.inventory.devices.len(), changes.added, changes.updated, changes.removed));
            }
            ui.add_space(18.0);
            let cards: Vec<_> = groups(&self.inventory.devices).into_iter()
                .filter(|g| g.members.iter().any(|d| (self.class.is_empty() || self.class == d.class) && d.matches(&self.search)))
                .map(|g| (g.key, g.members[0].title().to_owned(), g.members[0].class.clone(), g.members.len())).collect();
            if cards.is_empty() && self.inventory.last_good.is_some() { ui.label("No devices match this view."); }
            egui::ScrollArea::vertical().id_salt("cards").show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(16.0);
                ui.horizontal_wrapped(|ui| {
                    for (key, name, class, count) in cards {
                        let text = format!("\n\n\n\n\n{name}\n{class}\nInformation only · {count} OS record(s)");
                        let response = ui.add_sized([224.0, 236.0], egui::Button::new(RichText::new(text).size(13.0)).selected(self.selected.as_ref() == Some(&key)));
                        draw_device(ui, response.rect, &class, if self.prefs.light { Color32::from_rgb(43, 70, 52) } else { ACCENT });
                        if response.clicked() { self.selected = Some(key); }
                    }
                });
            });
        });
        self.preferences(ctx);
    }
}

fn draw_device(ui: &egui::Ui, rect: egui::Rect, class: &str, color: Color32) {
    let center = rect.center_top() + Vec2::new(0.0, 66.0);
    let painter = ui.painter();
    let stroke = Stroke::new(2.0, color);
    let kind = class.to_ascii_lowercase();
    if kind.contains("mouse") {
        let outline = egui::Rect::from_center_size(center, Vec2::new(48.0, 76.0));
        painter.rect_stroke(outline, 22.0, stroke, egui::StrokeKind::Inside);
        painter.line_segment([center - Vec2::new(0.0, 36.0), center], stroke);
        painter.line_segment(
            [
                center - Vec2::new(22.0, 6.0),
                center + Vec2::new(22.0, -6.0),
            ],
            stroke,
        );
    } else if kind.contains("keyboard") {
        painter.rect_stroke(
            egui::Rect::from_center_size(center, Vec2::new(130.0, 55.0)),
            5.0,
            stroke,
            egui::StrokeKind::Inside,
        );
        for row in 0..3 {
            for col in 0..10 {
                let key = center + Vec2::new(-54.0 + col as f32 * 12.0, -16.0 + row as f32 * 12.0);
                painter.rect_filled(
                    egui::Rect::from_center_size(key, Vec2::splat(7.0)),
                    1.0,
                    color,
                );
            }
        }
    } else {
        painter.rect_stroke(
            egui::Rect::from_center_size(center, Vec2::new(94.0, 60.0)),
            6.0,
            stroke,
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [center + Vec2::new(0.0, 30.0), center + Vec2::new(0.0, 44.0)],
            stroke,
        );
        painter.line_segment(
            [
                center + Vec2::new(-22.0, 44.0),
                center + Vec2::new(22.0, 44.0),
            ],
            stroke,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_validate_before_saving() {
        assert_eq!(
            parse_preferences("device-lab-preferences-v1\ntheme=dark\nscale=1\n"),
            Ok(Preferences::default())
        );
        for text in [
            "garbage",
            "device-lab-preferences-v1\ntheme=dark\nscale=NaN",
            "device-lab-preferences-v1\ntheme=light\nscale=99",
        ] {
            assert!(parse_preferences(text).is_err());
        }
    }

    #[test]
    fn pending_slot_replaces_old_snapshot() {
        let slot = Mutex::new(Some(Ok::<_, String>(vec![
            crate::devices::Device::default(),
        ])));
        *slot.lock().unwrap() = Some(Ok(vec![]));
        assert!(slot.lock().unwrap().take().unwrap().unwrap().is_empty());
    }
}
