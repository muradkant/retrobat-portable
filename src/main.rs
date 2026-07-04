#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;
use retrobat_portable::artwork::{load_or_fetch, load_snapshot_artwork};
use retrobat_portable::browse::{BrowseCatalog, BrowseEntry, InstallState};
use retrobat_portable::catalog::{Catalog, CatalogEntry};
use retrobat_portable::install::{Installer, ReqwestDownloader, is_installed};
use retrobat_portable::launch::LaunchPlan;
use retrobat_portable::paths::PortableLayout;

fn main() -> eframe::Result {
    let mut bundle_root = std::env::current_exe()
        .ok()
        .map(|path| PortableLayout::from_executable(&path).root)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut self_check_only = false;
    let mut self_check_output = None;
    let mut install_id = None;
    let mut uninstall_id = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--bundle-root" => {
                if let Some(value) = args.next() {
                    bundle_root = PathBuf::from(value);
                }
            }
            "--self-check" => self_check_only = true,
            "--install" => {
                install_id = args.next();
                if install_id.is_none() {
                    eprintln!("--install requires a catalog id");
                    std::process::exit(2);
                }
            }
            "--uninstall" => {
                uninstall_id = args.next();
                if uninstall_id.is_none() {
                    eprintln!("--uninstall requires a catalog id");
                    std::process::exit(2);
                }
            }
            "--self-check-output" => {
                self_check_output = args.next().map(PathBuf::from);
                if self_check_output.is_none() {
                    eprintln!("--self-check-output requires a path");
                    std::process::exit(2);
                }
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let layout = PortableLayout::new(bundle_root);
    if self_check_only {
        match retrobat_portable::self_check(&layout) {
            Ok(report) => {
                let json = serde_json::to_string_pretty(&report).unwrap();
                if let Some(path) = self_check_output
                    && let Err(error) = std::fs::write(&path, format!("{json}\n"))
                {
                    eprintln!(
                        "Could not write self-check report to {}: {error}",
                        path.display()
                    );
                    std::process::exit(1);
                }
                println!("{json}");
                return Ok(());
            }
            Err(error) => {
                eprintln!("Self-check failed: {error}");
                std::process::exit(1);
            }
        }
    }
    if install_id.is_some() || uninstall_id.is_some() {
        let catalog = Catalog::built_in().unwrap_or_else(|error| {
            eprintln!("Catalog rejected: {error}");
            std::process::exit(1);
        });
        let requested_id = install_id.as_ref().or(uninstall_id.as_ref()).unwrap();
        let entry = catalog
            .entries
            .iter()
            .find(|entry| &entry.id == requested_id)
            .unwrap_or_else(|| {
                eprintln!("Unknown catalog id: {requested_id}");
                std::process::exit(2);
            });
        let downloader = ReqwestDownloader::new().unwrap_or_else(|error| {
            eprintln!("Could not initialize downloader: {error}");
            std::process::exit(1);
        });
        let installer = Installer::new(&layout, &downloader);
        if install_id.is_some() {
            match installer.install(entry) {
                Ok(report) => println!(
                    "Installed {} bytes at {} (SHA-256 {}).",
                    report.bytes,
                    report.destination.display(),
                    report.sha256
                ),
                Err(error) => {
                    eprintln!("Install failed safely: {error}");
                    std::process::exit(1);
                }
            }
        } else {
            match installer.uninstall(entry) {
                Ok(report) => println!(
                    "Removed {} file(s); preserved {} modified file(s).",
                    report.removed.len(),
                    report.preserved_modified.len()
                ),
                Err(error) => {
                    eprintln!("Uninstall failed safely: {error}");
                    std::process::exit(1);
                }
            }
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([680.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RetroBat Portable",
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(PortableApp::new(
                layout,
                &creation_context.egui_ctx,
            )))
        }),
    )
}

struct ArtworkMessage {
    entry_id: String,
    result: Result<Vec<u8>, String>,
}

struct PortableApp {
    root_text: String,
    catalog: Catalog,
    status: String,
    operation: Option<Receiver<String>>,
    artwork_sender: mpsc::Sender<ArtworkMessage>,
    artwork_receiver: Receiver<ArtworkMessage>,
    textures: HashMap<String, egui::TextureHandle>,
    artwork_errors: HashMap<String, String>,
    artwork_pending: usize,
    artwork_inflight: HashSet<String>,
    browse: BrowseCatalog,
    browse_page: usize,
    system_filter: String,
    search: String,
}

impl PortableApp {
    fn new(layout: PortableLayout, context: &egui::Context) -> Self {
        #[cfg(target_os = "windows")]
        if context.native_pixels_per_point().unwrap_or(1.0) < 1.15 {
            // Wine and some 100%-scaled Windows desktops report 96 DPI even on
            // dense displays. Keep the library comfortably readable while
            // leaving Windows' 125%+ accessibility scaling untouched.
            context.set_zoom_factor(1.2);
        }

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(13, 16, 23);
        visuals.window_fill = egui::Color32::from_rgb(18, 22, 31);
        visuals.selection.bg_fill = egui::Color32::from_rgb(58, 113, 255);
        context.set_visuals(visuals);

        let (catalog, status) = match Catalog::built_in() {
            Ok(catalog) => {
                let count = catalog.entries.len();
                (
                    catalog,
                    format!("Verified catalog loaded: {count} item(s)."),
                )
            }
            Err(error) => (
                Catalog {
                    schema_version: 1,
                    generated_at: String::new(),
                    entries: Vec::new(),
                },
                format!("Catalog rejected: {error}"),
            ),
        };
        let (artwork_sender, artwork_receiver) = mpsc::channel();
        let mut artwork_pending = 0;
        for entry in catalog.entries.iter().cloned() {
            let Some(artwork) = entry.artwork.first().cloned() else {
                continue;
            };
            artwork_pending += 1;
            let sender = artwork_sender.clone();
            let layout = layout.clone();
            thread::spawn(move || {
                let result = ReqwestDownloader::new()
                    .map_err(|error| error.to_string())
                    .and_then(|downloader| {
                        load_or_fetch(&layout, &artwork, &downloader)
                            .map_err(|error| error.to_string())
                    });
                let _ = sender.send(ArtworkMessage {
                    entry_id: entry.id,
                    result,
                });
            });
        }
        let browse = BrowseCatalog::homebrew_hub().unwrap_or_else(|error| {
            eprintln!("Browse snapshot rejected: {error}");
            BrowseCatalog {
                schema_version: 1,
                generated_at: String::new(),
                source: retrobat_portable::browse::BrowseSource {
                    id: "unavailable".into(),
                    name: "Unavailable".into(),
                    homepage: String::new(),
                },
                entries: Vec::new(),
            }
        });
        Self {
            root_text: layout.root.display().to_string(),
            catalog,
            status,
            operation: None,
            artwork_sender,
            artwork_receiver,
            textures: HashMap::new(),
            artwork_errors: HashMap::new(),
            artwork_pending,
            artwork_inflight: HashSet::new(),
            browse,
            browse_page: 0,
            system_filter: "all".into(),
            search: String::new(),
        }
    }

    fn start_browse_artwork(&mut self, requests: Vec<(String, String)>) {
        let layout = PortableLayout::new(PathBuf::from(&self.root_text));
        for (entry_id, url) in requests {
            if !self.artwork_inflight.insert(entry_id.clone()) {
                continue;
            }
            self.artwork_pending += 1;
            let sender = self.artwork_sender();
            let layout = layout.clone();
            thread::spawn(move || {
                let result = ReqwestDownloader::new()
                    .map_err(|error| error.to_string())
                    .and_then(|downloader| {
                        load_snapshot_artwork(&layout, &url, &downloader)
                            .map_err(|error| error.to_string())
                    });
                let _ = sender.send(ArtworkMessage { entry_id, result });
            });
        }
    }

    fn artwork_sender(&self) -> mpsc::Sender<ArtworkMessage> {
        self.artwork_sender.clone()
    }

    fn start_install(&mut self, entry: CatalogEntry) {
        let layout = PortableLayout::new(PathBuf::from(&self.root_text));
        let (sender, receiver) = mpsc::channel();
        self.operation = Some(receiver);
        self.status = format!("Downloading and verifying {}…", entry.title);
        thread::spawn(move || {
            let result = ReqwestDownloader::new()
                .and_then(|downloader| Installer::new(&layout, &downloader).install(&entry))
                .map(|report| {
                    format!(
                        "Installed and verified {} bytes at {}.",
                        report.bytes,
                        report.destination.display()
                    )
                })
                .unwrap_or_else(|error| format!("Install failed safely: {error}"));
            let _ = sender.send(result);
        });
    }

    fn start_uninstall(&mut self, entry: CatalogEntry) {
        let layout = PortableLayout::new(PathBuf::from(&self.root_text));
        let (sender, receiver) = mpsc::channel();
        self.operation = Some(receiver);
        self.status = format!("Checking and removing {}…", entry.title);
        thread::spawn(move || {
            let result = ReqwestDownloader::new()
                .and_then(|downloader| Installer::new(&layout, &downloader).uninstall(&entry))
                .map(|report| {
                    if report.preserved_modified.is_empty() {
                        format!("Removed {} owned file(s).", report.removed.len())
                    } else {
                        format!(
                            "Preserved {} modified file(s); no user changes were deleted.",
                            report.preserved_modified.len()
                        )
                    }
                })
                .unwrap_or_else(|error| format!("Uninstall failed safely: {error}"));
            let _ = sender.send(result);
        });
    }
}

impl eframe::App for PortableApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(receiver) = &self.operation
            && let Ok(status) = receiver.try_recv()
        {
            self.status = status;
            self.operation = None;
        }
        if self.operation.is_some() {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }

        while let Ok(message) = self.artwork_receiver.try_recv() {
            self.artwork_pending = self.artwork_pending.saturating_sub(1);
            self.artwork_inflight.remove(&message.entry_id);
            match message.result {
                Ok(bytes) => match decode_artwork(&bytes) {
                    Ok(decoded) => {
                        let rgba = decoded.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let color_image =
                            egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        let texture = context.load_texture(
                            format!("artwork/{}", message.entry_id),
                            color_image,
                            egui::TextureOptions::NEAREST,
                        );
                        self.textures.insert(message.entry_id, texture);
                    }
                    Err(error) => {
                        self.artwork_errors
                            .insert(message.entry_id, format!("Image decode failed: {error}"));
                    }
                },
                Err(error) => {
                    self.artwork_errors.insert(message.entry_id, error);
                }
            }
            context.request_repaint();
        }
        if self.artwork_pending > 0 {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        const ACCENT: egui::Color32 = egui::Color32::from_rgb(104, 146, 255);
        const CARD: egui::Color32 = egui::Color32::from_rgb(22, 27, 38);
        let panel = egui::Frame::new()
            .fill(egui::Color32::from_rgb(13, 16, 23))
            .inner_margin(24);
        egui::CentralPanel::default().frame(panel).show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("RETRO")
                        .size(28.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.label(
                    egui::RichText::new("// PORTABLE")
                        .size(28.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let layout = PortableLayout::new(PathBuf::from(&self.root_text));
                    let can_launch = layout.retrobat_executable().is_file();
                    if ui
                        .add_enabled(
                            can_launch && self.operation.is_none(),
                            egui::Button::new(
                                egui::RichText::new("▶  PLAY LIBRARY")
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(ACCENT)
                            .min_size(egui::vec2(150.0, 38.0)),
                        )
                        .clicked()
                    {
                        self.status = match LaunchPlan::for_current_host(&layout)
                            .and_then(|plan| plan.spawn().map(|_| ()))
                        {
                            Ok(()) => "RetroBat launched.".to_owned(),
                            Err(error) => format!("Launch failed: {error}"),
                        };
                    }
                    let state = if can_launch { "READY" } else { "SETUP NEEDED" };
                    ui.label(
                        egui::RichText::new(state)
                            .small()
                            .strong()
                            .color(if can_launch {
                                egui::Color32::from_rgb(98, 211, 145)
                            } else {
                                egui::Color32::from_rgb(238, 177, 89)
                            }),
                    );
                });
            });
            ui.label(
                egui::RichText::new("A visual library of verified, legally distributed classics.")
                    .size(15.0)
                    .color(egui::Color32::from_gray(165)),
            );
            egui::ScrollArea::vertical()
                .id_salt("library-body")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(20.0);

                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("Search games, systems, or developers…")
                                .desired_width(420.0),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} VERIFIED GAME{}",
                                self.catalog.entries.len(),
                                if self.catalog.entries.len() == 1 {
                                    ""
                                } else {
                                    "S"
                                }
                            ))
                            .small()
                            .color(egui::Color32::from_gray(145)),
                        );
                    });
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("HOMEBREW HUB")
                                .size(18.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} TITLES", self.browse.entries.len()))
                                .small()
                                .color(egui::Color32::from_gray(135)),
                        );
                        for system in ["all", "gb", "gbc", "gba", "nes"] {
                            let selected = self.system_filter == system;
                            if ui
                                .selectable_label(selected, system.to_ascii_uppercase())
                                .clicked()
                            {
                                self.system_filter = system.to_owned();
                                self.browse_page = 0;
                            }
                        }
                    });
                    ui.label(
                egui::RichText::new(
                    "Browse every source entry visually. Installation badges remain trust-gated.",
                )
                .small()
                .color(egui::Color32::from_gray(140)),
            );
                    ui.add_space(8.0);

                    const BROWSE_PAGE_SIZE: usize = 12;
                    let query = self.search.trim().to_ascii_lowercase();
                    let matches: Vec<BrowseEntry> = self
                        .browse
                        .entries
                        .iter()
                        .filter(|entry| {
                            (self.system_filter == "all" || entry.system == self.system_filter)
                                && (query.is_empty()
                                    || entry.title.to_ascii_lowercase().contains(&query)
                                    || entry.developer.to_ascii_lowercase().contains(&query)
                                    || entry.system.to_ascii_lowercase().contains(&query)
                                    || entry
                                        .tags
                                        .iter()
                                        .any(|tag| tag.to_ascii_lowercase().contains(&query)))
                        })
                        .cloned()
                        .collect();
                    let page_count = matches.len().div_ceil(BROWSE_PAGE_SIZE).max(1);
                    let match_count = matches.len();
                    self.browse_page = self.browse_page.min(page_count - 1);
                    let page_start = self.browse_page * BROWSE_PAGE_SIZE;
                    let page_entries: Vec<BrowseEntry> = matches
                        .into_iter()
                        .skip(page_start)
                        .take(BROWSE_PAGE_SIZE)
                        .collect();
                    let mut artwork_requests = Vec::new();

                    egui::ScrollArea::horizontal()
                        .id_salt("browse-cards")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for entry in &page_entries {
                                    egui::Frame::new()
                                        .fill(CARD)
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            egui::Color32::from_rgb(43, 51, 69),
                                        ))
                                        .corner_radius(10)
                                        .inner_margin(10)
                                        .show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.set_width(180.0);
                                                if let Some(texture) = self.textures.get(&entry.id)
                                                {
                                                    ui.add(
                                                        egui::Image::new((
                                                            texture.id(),
                                                            egui::vec2(180.0, 120.0),
                                                        ))
                                                        .fit_to_exact_size(egui::vec2(180.0, 120.0))
                                                        .corner_radius(7),
                                                    );
                                                } else {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        egui::vec2(180.0, 120.0),
                                                        egui::Sense::hover(),
                                                    );
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        7,
                                                        egui::Color32::from_rgb(28, 35, 49),
                                                    );
                                                    ui.painter().text(
                                                        rect.center(),
                                                        egui::Align2::CENTER_CENTER,
                                                        if entry.artwork_url.is_some() {
                                                            "LOADING..."
                                                        } else {
                                                            "NO ARTWORK"
                                                        },
                                                        egui::FontId::proportional(12.0),
                                                        egui::Color32::from_gray(130),
                                                    );
                                                    if let Some(url) = &entry.artwork_url
                                                        && !self
                                                            .artwork_inflight
                                                            .contains(&entry.id)
                                                        && !self
                                                            .artwork_errors
                                                            .contains_key(&entry.id)
                                                        && !self
                                                            .catalog
                                                            .entries
                                                            .iter()
                                                            .any(|trusted| trusted.id == entry.id)
                                                    {
                                                        artwork_requests
                                                            .push((entry.id.clone(), url.clone()));
                                                    }
                                                }
                                                ui.add_space(5.0);
                                                ui.label(
                                                    egui::RichText::new(&entry.title)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{} / {}",
                                                        entry.system.to_ascii_uppercase(),
                                                        entry.developer
                                                    ))
                                                    .small()
                                                    .color(egui::Color32::from_gray(145)),
                                                );
                                                let (trust, trust_color) = match entry.install_state
                                                {
                                                    InstallState::Verified => (
                                                        "VERIFIED INSTALL",
                                                        egui::Color32::from_rgb(98, 211, 145),
                                                    ),
                                                    InstallState::AuditRequired => (
                                                        "AWAITING ARTIFACT AUDIT",
                                                        egui::Color32::from_rgb(238, 177, 89),
                                                    ),
                                                    InstallState::BrowseOnly => (
                                                        "BROWSE ONLY",
                                                        egui::Color32::from_gray(125),
                                                    ),
                                                };
                                                ui.label(
                                                    egui::RichText::new(trust)
                                                        .small()
                                                        .strong()
                                                        .color(trust_color),
                                                );
                                            });
                                        });
                                }
                            });
                        });
                    self.start_browse_artwork(artwork_requests);

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(self.browse_page > 0, egui::Button::new("PREVIOUS"))
                            .clicked()
                        {
                            self.browse_page -= 1;
                        }
                        ui.label(format!(
                            "Page {} of {} / {} matching titles",
                            self.browse_page + 1,
                            page_count,
                            match_count
                        ));
                        if ui
                            .add_enabled(
                                self.browse_page + 1 < page_count,
                                egui::Button::new("NEXT"),
                            )
                            .clicked()
                        {
                            self.browse_page += 1;
                        }
                    });
                    ui.add_space(14.0);
                    ui.label(
                        egui::RichText::new("VERIFIED INSTALLS")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.add_space(6.0);

                    let mut requested_action: Option<(bool, CatalogEntry)> = None;
                    let layout = PortableLayout::new(PathBuf::from(&self.root_text));
                    for entry in &self.catalog.entries {
                        if !query.is_empty()
                            && !entry.title.to_ascii_lowercase().contains(&query)
                            && !entry.developer.to_ascii_lowercase().contains(&query)
                            && !entry.system.to_ascii_lowercase().contains(&query)
                        {
                            continue;
                        }
                        egui::Frame::new()
                            .fill(CARD)
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(43, 51, 69)))
                            .corner_radius(14)
                            .inner_margin(16)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(texture) = self.textures.get(&entry.id) {
                                        ui.add(
                                            egui::Image::new((
                                                texture.id(),
                                                egui::vec2(320.0, 288.0),
                                            ))
                                            .corner_radius(10),
                                        );
                                    } else {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2(320.0, 288.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            rect,
                                            10,
                                            egui::Color32::from_rgb(28, 35, 49),
                                        );
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            if self.artwork_errors.contains_key(&entry.id) {
                                                "ARTWORK UNAVAILABLE"
                                            } else {
                                                "LOADING ARTWORK…"
                                            },
                                            egui::FontId::proportional(14.0),
                                            egui::Color32::from_gray(135),
                                        );
                                    }

                                    ui.add_space(22.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(
                                                    entry.system.to_ascii_uppercase(),
                                                )
                                                .small()
                                                .strong()
                                                .color(ACCENT),
                                            );
                                            ui.label(
                                                egui::RichText::new("VERIFIED SOURCE")
                                                    .small()
                                                    .strong()
                                                    .color(egui::Color32::from_rgb(98, 211, 145)),
                                            );
                                        });
                                        ui.add_space(10.0);
                                        ui.label(
                                            egui::RichText::new(&entry.title)
                                                .size(34.0)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("by {}", entry.developer))
                                                .size(16.0)
                                                .color(egui::Color32::from_gray(170)),
                                        );
                                        ui.add_space(14.0);
                                        ui.label(
                                            egui::RichText::new(&entry.description)
                                                .size(17.0)
                                                .color(egui::Color32::from_gray(215)),
                                        );
                                        ui.add_space(18.0);

                                        let installed = is_installed(&layout, entry);
                                        let (label, color) = if installed {
                                            (
                                                "INSTALLED - REMOVE",
                                                egui::Color32::from_rgb(54, 62, 78),
                                            )
                                        } else {
                                            ("+  INSTALL GAME", ACCENT)
                                        };
                                        if ui
                                            .add_enabled(
                                                self.operation.is_none(),
                                                egui::Button::new(
                                                    egui::RichText::new(label)
                                                        .strong()
                                                        .color(egui::Color32::WHITE),
                                                )
                                                .fill(color)
                                                .min_size(egui::vec2(210.0, 44.0)),
                                            )
                                            .clicked()
                                        {
                                            requested_action = Some((installed, entry.clone()));
                                        }
                                        ui.add_space(16.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}  •  {} KiB  •  SHA-256 checked",
                                                entry.license.name,
                                                entry.artifact.size / 1024
                                            ))
                                            .small()
                                            .color(egui::Color32::from_gray(145)),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Artwork and game metadata: {}",
                                                entry.source.name
                                            ))
                                            .small()
                                            .color(egui::Color32::from_gray(115)),
                                        );
                                    });
                                });
                            });
                    }
                    if let Some((uninstall, entry)) = requested_action {
                        if uninstall {
                            self.start_uninstall(entry);
                        } else {
                            self.start_install(entry);
                        }
                    }

                    ui.add_space(12.0);
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(17, 21, 29))
                        .corner_radius(8)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&self.status)
                                    .small()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        });

                    ui.add_space(8.0);
                    ui.collapsing("Bundle settings", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Portable bundle:");
                            ui.add_enabled(
                                self.operation.is_none(),
                                egui::TextEdit::singleline(&mut self.root_text)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                        if !layout.retrobat_executable().is_file() {
                            ui.label("Expected RetroBat/RetroBat.exe inside this folder.");
                        }
                    });
                });
        });
    }
}

fn decode_artwork(bytes: &[u8]) -> image::ImageResult<image::DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4_096);
    limits.max_image_height = Some(4_096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()
}
