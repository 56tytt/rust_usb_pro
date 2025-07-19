// rustburn-gui/src/main.rs

// Import necessary crates and modules.
use eframe::{NativeOptions, egui};
use egui::{FontData, FontDefinitions, FontFamily, TextureHandle, Visuals};

use rfd; // For file dialogs.
use rustburn_core::{BootType, BurnOptions, RustBurn, UiProgress, UsbDevice};
use std::sync::mpsc;
use std::thread;

/// This struct holds the loaded image textures for our icons.
struct AppIcons {
    add: TextureHandle,
    burn: TextureHandle,
    scan: TextureHandle,
    stop: TextureHandle,
    remove: TextureHandle,
    clear: TextureHandle,
    win_iso: TextureHandle,
}

impl AppIcons {
    /// Creates a new instance of `AppIcons` by loading images from bytes.
    fn new(ctx: &egui::Context) -> Self {
        Self {
            add: load_icon(ctx, "add_icon", include_bytes!("../assets/fd.png")),
            burn: load_icon(ctx, "burn_icon", include_bytes!("../assets/fl.png")),
            scan: load_icon(ctx, "scan_icon", include_bytes!("../assets/rad.png")),
            stop: load_icon(ctx, "stop_icon", include_bytes!("../assets/st.png")),
            remove: load_icon(ctx, "remove_icon", include_bytes!("../assets/rm.png")),
            clear: load_icon(ctx, "clear_icon", include_bytes!("../assets/cl.png")),
            win_iso: load_icon(ctx, "win_iso_icon", include_bytes!("../assets/wi.png")),
        }
    }
}

/// This enum represents the current state of the application.
#[derive(PartialEq, Debug)]
enum AppStatus {
    Idle,
    Scanning,
    Burning,
    CreatingWinIso,
    Verifying,
    SettingUpBootable,
    Ejecting,
    Erasing,
    Done,
    Error(String),
}

/// This is the main struct that holds our application's state.
struct RustBurnApp {
    is_dark_mode: bool,
    icons: AppIcons,
    devices: Vec<UsbDevice>,
    burn_options: BurnOptions,
    selected_device: Option<String>,
    status: AppStatus,
    burn_progress: f32,
    /// The type here is now corrected to use the unified `UiProgress`.
    progress_receiver: Option<mpsc::Receiver<UiProgress>>,
    /// Use the correct field name for the background operation thread.
    operation_thread: Option<thread::JoinHandle<()>>,
    show_about_window: bool,
    is_file_hovering: bool,
    show_log_panel: bool,
    logs: Vec<String>,
}

impl RustBurnApp {
    /// This function is called once to create the application state.
    fn new(cc: &eframe::CreationContext) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        Self {
            is_dark_mode: true,
            icons: AppIcons::new(&cc.egui_ctx),
            devices: Vec::new(),
            burn_options: BurnOptions::default(),
            selected_device: None,
            status: AppStatus::Idle,
            burn_progress: 0.0,
            progress_receiver: None,
            operation_thread: None,
            show_about_window: false,
            is_file_hovering: false,
            // The comma was missing after the line above this one.
            show_log_panel: false,
            logs: Vec::new(),
        }
    }
}

// In rustburn-gui/src/main.rs, replace the entire `update` function.
impl eframe::App for RustBurnApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for progress updates from the background thread.
        if let Some(rx) = &self.progress_receiver {
            while let Ok(update) = rx.try_recv() {
                // This match block now covers all possible UiProgress variants.
                match update {
                    UiProgress::Log(msg) => self.logs.push(msg),
                    UiProgress::StartingBurn => self.status = AppStatus::Burning,
                    UiProgress::Writing(p) => self.burn_progress = p,
                    UiProgress::StartingVerification => self.status = AppStatus::Verifying,
                    UiProgress::Verifying(p) => self.burn_progress = p,
                    UiProgress::StartingBootableSetup => self.status = AppStatus::SettingUpBootable,
                    UiProgress::StartingCreateWinIso => self.status = AppStatus::CreatingWinIso,
                    UiProgress::StartingEject => self.status = AppStatus::Ejecting,
                    UiProgress::StartingErase => self.status = AppStatus::Erasing,
                    UiProgress::Done => {
                        self.status = AppStatus::Done;
                        self.operation_thread = None;
                    }
                    UiProgress::Error(e) => {
                        self.logs.push(format!("ERROR: {}", e));
                        self.status = AppStatus::Error(e);
                        self.operation_thread = None;
                    }
                }
            }
        }

        // Set the visual theme (dark/light).
        ctx.set_visuals(if self.is_dark_mode {
            Visuals::dark()
        } else {
            Visuals::light()
        });

        // Render the different parts of the UI.
        self.render_top_panel(ctx);
        self.render_central_panel(ctx);
        self.render_bottom_panel(ctx);
        self.render_about_window(ctx);
        self.render_drag_and_drop_overlay(ctx);
        self.render_log_panel(ctx);

        // Keep redrawing the UI if an operation is active.
        if !self.is_idle() {
            ctx.request_repaint();
        }
    }
}

impl RustBurnApp {
    /// Renders the top panel of the GUI, including the menu bar, toolbar, and options.
    fn render_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // --- Menu Bar ---
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Scan Devices").clicked() {
                        self.scan_devices();
                    }
                    if ui.button("Select ISO...").clicked() {
                        self.select_iso_file();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Toggle Theme").clicked() {
                        self.is_dark_mode = !self.is_dark_mode;
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about_window = true;
                    }
                });
            });
            ui.separator();

            // --- Toolbar with action buttons ---
            ui.horizontal(|ui| {
                let is_idle =
                    self.status == AppStatus::Idle || matches!(self.status, AppStatus::Error(_));
                if ui
                    .add_enabled(is_idle, egui::ImageButton::new(&self.icons.scan))
                    .on_hover_text("Scan for devices")
                    .clicked()
                {
                    self.scan_devices();
                }
                ui.separator();
                if ui
                    .add_enabled(is_idle, egui::ImageButton::new(&self.icons.add))
                    .on_hover_text("Select ISO file")
                    .clicked()
                {
                    self.select_iso_file();
                }
                if ui
                    .add_enabled(is_idle, egui::ImageButton::new(&self.icons.clear))
                    .on_hover_text("Clear selections")
                    .clicked()
                {
                    self.burn_options.iso_path.clear();
                    self.selected_device = None;
                    self.devices.clear();
                }
                ui.separator();
                let can_burn =
                    self.selected_device.is_some() && !self.burn_options.iso_path.is_empty();
                if ui
                    .add_enabled(
                        can_burn && is_idle,
                        egui::ImageButton::new(&self.icons.burn),
                    )
                    .on_hover_text("Burn to device")
                    .clicked()
                {
                    self.start_burn();
                }
                if ui
                    .add_enabled(!is_idle, egui::ImageButton::new(&self.icons.stop))
                    .on_hover_text("Stop operation (Not Implemented)")
                    .clicked()
                {
                    // TODO: Implement stopping logic
                }

                if ui
                    .add_enabled(is_idle, egui::ImageButton::new(&self.icons.win_iso))
                    .on_hover_text("Create Windows ISO")
                    .clicked()
                {
                    self.start_create_win_iso();
                }
            });
            ui.separator();

            // --- Main Options Panel ---
            // We use a horizontal layout to contain our two grids.
            ui.columns(2, |columns| {
                // --- Left Section: Primary Options ---
                egui::Grid::new("primary_options_grid")
                    .num_columns(2)
                    .spacing([20.0, 8.0])
                    .show(&mut columns[0], |ui| {
                        // Row 1: Threads
                        ui.label("Threads:");
                        ui.add(egui::Slider::new(&mut self.burn_options.threads, 1..=16));
                        ui.end_row();

                        // Row 2: Bootable Options
                        ui.label("Bootable:");
                        ui.vertical(|ui| {
                            if ui
                                .checkbox(&mut self.burn_options.make_bootable, "Make bootable")
                                .clicked()
                                && !self.burn_options.make_bootable
                            {
                                // Reset to default if unchecked
                                self.burn_options.boot_type = BootType::Hybrid;
                            }

                            // Show ComboBox only if bootable is checked
                            ui.add_enabled_ui(self.burn_options.make_bootable, |ui| {
                                egui::ComboBox::from_id_source("boot_type_combo")
                                    .selected_text(format!("{:?}", self.burn_options.boot_type))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.burn_options.boot_type,
                                            BootType::UEFI,
                                            "UEFI",
                                        );
                                        ui.selectable_value(
                                            &mut self.burn_options.boot_type,
                                            BootType::Legacy,
                                            "Legacy",
                                        );
                                        ui.selectable_value(
                                            &mut self.burn_options.boot_type,
                                            BootType::Hybrid,
                                            "Hybrid",
                                        );
                                    });
                            });
                        });
                        ui.end_row();
                    });

                // --- Right Section: Advanced Options ---
                egui::Grid::new("advanced_options_grid")
                    .num_columns(2)
                    .spacing([20.0, 8.0])
                    .show(&mut columns[1], |ui| {
                        // Row 1: Verification
                        ui.label("Verification:");
                        ui.checkbox(&mut self.burn_options.verify, "Verify after burn");
                        ui.end_row();

                        // Row 2: Block Size
                        ui.label("Block Size:");
                        // A ComboBox is more user-friendly for predefined block sizes.
                        egui::ComboBox::from_id_source("block_size_combo")
                            .selected_text(format!("{} KB", self.burn_options.block_size / 1024))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.burn_options.block_size,
                                    512 * 1024,
                                    "512 KB",
                                );
                                ui.selectable_value(
                                    &mut self.burn_options.block_size,
                                    1024 * 1024,
                                    "1 MB",
                                );
                                ui.selectable_value(
                                    &mut self.burn_options.block_size,
                                    2048 * 1024,
                                    "2 MB",
                                );
                                ui.selectable_value(
                                    &mut self.burn_options.block_size,
                                    4096 * 1024,
                                    "4 MB",
                                );
                            });
                        ui.end_row();
                    });
            });
        });
    }

    /// Initiates the process of creating a Windows ISO in a background thread.
    fn start_create_win_iso(&mut self) {
        let source_folder = rfd::FileDialog::new().pick_folder();
        let save_file = rfd::FileDialog::new()
            .add_filter("ISO Image", &["iso"])
            .save_file();

        if let (Some(source), Some(output)) = (source_folder, save_file) {
            let (tx, rx) = mpsc::channel();
            self.progress_receiver = Some(rx);
            // Spawn the operation in a new thread to prevent UI freezing.
            self.operation_thread = Some(thread::spawn(move || {
                RustBurn::create_win_iso(
                    source.display().to_string(),
                    output.display().to_string(),
                    tx,
                );
            }));
            self.status = AppStatus::CreatingWinIso;
        }
    }

    // Add these two new functions inside the `impl RustBurnApp` block.

    /// Detects when files are hovered or dropped onto the window.
    fn detect_drag_and_drop(&mut self, ctx: &egui::Context) {
        // First, check for dropped files.
        if !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let files = ctx.input(|i| i.raw.dropped_files.clone());
            self.is_file_hovering = false; // The hover is over once a file is dropped.

            // Find the first valid .iso file from the dropped files.
            if let Some(file) = files.iter().find(|f| {
                if let Some(path) = &f.path {
                    return path.extension().map_or(false, |ext| ext == "iso");
                }
                false
            }) {
                if let Some(path) = &file.path {
                    self.burn_options.iso_path = path.display().to_string();
                }
            }
            return; // Stop processing to avoid flicker.
        }

        // Next, check if files are being hovered over the window.
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            self.is_file_hovering = true;
        } else {
            self.is_file_hovering = false;
        }
    }

    /// Renders a semi-transparent overlay when a file is being hovered over the window.
    fn render_drag_and_drop_overlay(&mut self, ctx: &egui::Context) {
        if !self.is_file_hovering {
            return;
        }

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("dnd_overlay"),
        ));
        let screen_rect = ctx.screen_rect();

        // Draw a semi-transparent black rectangle to dim the background UI.
        painter.add(egui::Shape::rect_filled(
            screen_rect,
            // This is the corrected line:
            egui::Rounding::ZERO,
            egui::Color32::from_black_alpha(128),
        ));

        // Draw the instructional text in the center of the screen.
        painter.text(
            screen_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drop ISO file here",
            egui::FontId::proportional(40.0),
            egui::Color32::WHITE,
        );
    }

    // In rustburn-gui/src/main.rs, replace the existing render_about_window function.
    fn render_about_window(&mut self, ctx: &egui::Context) {
        // The .open() method handles the closing logic for us,
        // which resolves the double borrow error.
        egui::Window::new("About RustBurn Professional")
            .open(&mut self.show_about_window)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("RustBurn Professional");
                    ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
                    ui.hyperlink("https://github.com/56tytt");
                });
                ui.separator();
                ui.label("A professional, multi-threaded ISO burning utility,");
                ui.label("engineered by our elite software team.");
                ui.label("Shay Kadosh Software Engineering from Ashkelon")
            });
    }

    /// Renders the central panel, showing selected ISO and device list.
    fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("1. Selected ISO File");
            ui.label(if self.burn_options.iso_path.is_empty() {
                "No file selected."
            } else {
                &self.burn_options.iso_path
            });
            ui.add_space(10.0);

            ui.heading("2. Select Target Device");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for device in &self.devices {
                    let is_selected = self.selected_device.as_deref() == Some(&device.device);
                    let label = format!(
                        "{}  ({} {}) - {:.1} GB",
                        device.device,
                        device.vendor,
                        device.model,
                        device.size as f64 / 1e9
                    );
                    if ui.selectable_label(is_selected, label).clicked() {
                        self.selected_device = Some(device.device.clone());
                    }
                }
            });
        });
    }

    // In rustburn-gui/src/main.rs, replace the entire `render_bottom_panel` function.

    // In rustburn-gui/src/main.rs, replace the entire `render_bottom_panel` function.
    fn render_bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status_text = match &self.status {
                    AppStatus::Idle => "Ready".to_string(),
                    AppStatus::Scanning => "Scanning for devices...".to_string(),
                    AppStatus::Burning => format!("Burning... {:.0}%", self.burn_progress * 100.0),
                    AppStatus::CreatingWinIso => "Creating Windows ISO...".to_string(),
                    AppStatus::Verifying => {
                        format!("Verifying... {:.0}%", self.burn_progress * 100.0)
                    }
                    AppStatus::SettingUpBootable => "Making device bootable...".to_string(),
                    AppStatus::Ejecting => "Ejecting device...".to_string(),
                    AppStatus::Erasing => "Erasing device...".to_string(),
                    AppStatus::Done => "Operation completed successfully.".to_string(),
                    AppStatus::Error(e) => format!("Error: {}", e),
                };
                ui.label(status_text);

                if matches!(self.status, AppStatus::Burning | AppStatus::Verifying) {
                    ui.add(egui::ProgressBar::new(self.burn_progress).animate(true));
                } else if !self.is_idle() && self.status != AppStatus::Done {
                    // This is the corrected way to add a spinner.
                    ui.spinner();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("📜 Logs")
                        .on_hover_text("Show/Hide Logs")
                        .clicked()
                    {
                        self.show_log_panel = !self.show_log_panel;
                    }
                });
            });
        });
    }

    // --- Action Methods ---

    fn render_log_panel(&mut self, ctx: &egui::Context) {
        if self.show_log_panel {
            egui::TopBottomPanel::bottom("log_panel")
                .resizable(true)
                .default_height(150.0)
                .min_height(50.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("Logs");
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for log in &self.logs {
                                ui.monospace(log);
                            }
                        });
                });
        }
    }

    /// Scans for available USB devices.
    fn scan_devices(&mut self) {
        self.status = AppStatus::Scanning;
        self.devices = RustBurn::scan_devices().unwrap_or_else(|e| {
            self.status = AppStatus::Error(e.to_string());
            Vec::new()
        });
        // If scanning was successful, revert to Idle status.
        if self.status == AppStatus::Scanning {
            self.status = AppStatus::Idle;
        }
    }

    /// Opens a file dialog to select an ISO file.
    fn select_iso_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ISO Image", &["iso"])
            .pick_file()
        {
            self.burn_options.iso_path = path.display().to_string();
        }
    }

    /// Starts the ISO burning process in a background thread.
    fn start_burn(&mut self) {
        if let Some(device) = self.selected_device.clone() {
            self.burn_options.device_path = device;
            let (tx, rx) = mpsc::channel();
            self.progress_receiver = Some(rx);
            let burn_options_clone = self.burn_options.clone(); // Clone for the thread
            // Spawn the operation in a new thread to prevent UI freezing.
            self.operation_thread = Some(thread::spawn(move || {
                RustBurn::burn_iso(burn_options_clone, tx);
            }));
            self.status = AppStatus::Burning;
            self.burn_progress = 0.0;
        }
    }

    fn is_idle(&self) -> bool {
        matches!(
            self.status,
            AppStatus::Idle | AppStatus::Done | AppStatus::Error(_)
        )
    }
}

// --- Helper Functions ---

/// Loads an image from bytes and converts it into an egui `TextureHandle`.
fn load_icon(ctx: &egui::Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = image::load_from_memory(bytes).expect("Failed to load icon");
    let size = [image.width() as _, image.height() as _];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    ctx.load_texture(name, color_image, Default::default())
}

/// Sets up custom fonts for the egui context.
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "my_font".to_owned(),
        FontData::from_static(include_bytes!("../assets/rob.ttf")),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "my_font".to_owned());
    ctx.set_fonts(fonts);
}

/// The main entry point of the application.
fn main() {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RustBurn Professional",
        options,
        Box::new(|cc| Box::new(RustBurnApp::new(cc))),
    )
    .expect("Failed to run eframe");
}
