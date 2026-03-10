// RDirStat - GUI Application State
// License: GPL-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;

use rdirstat_core::dir_tree::DirTree;
use rdirstat_scan::{ScanOptions, ScanProgress, Scanner};

use crate::tree_view::TreeViewState;

pub enum ScanMessage {
    Finished(anyhow::Result<DirTree>),
}

pub struct RDirStatApp {
    // State
    current_path: Option<PathBuf>,
    is_scanning: bool,
    
    // Data
    dir_tree: Option<Arc<DirTree>>,
    
    // Concurrency
    scan_progress: Option<Arc<ScanProgress>>,
    scan_receiver: Option<Receiver<ScanMessage>>,
    
    // UI Components
    tree_view_state: TreeViewState,
}

impl Default for RDirStatApp {
    fn default() -> Self {
        Self {
            current_path: None,
            is_scanning: false,
            dir_tree: None,
            scan_progress: None,
            scan_receiver: None,
            tree_view_state: TreeViewState::default(),
        }
    }
}

impl RDirStatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn start_scan(&mut self, path: PathBuf, ctx: egui::Context) {
        self.is_scanning = true;
        self.current_path = Some(path.clone());
        self.dir_tree = None;

        let options = ScanOptions {
            path,
            ..Default::default()
        };

        let scanner = Scanner::new(options);
        self.scan_progress = Some(scanner.progress());

        let (sender, receiver) = unbounded();
        self.scan_receiver = Some(receiver);

        thread::spawn(move || {
            let result = scanner.scan();
            let _ = sender.send(ScanMessage::Finished(result));
            ctx.request_repaint(); // Wake UI thread
        });
    }

    fn update_scan_status(&mut self) {
        if !self.is_scanning {
            return;
        }

        if let Some(rx) = &self.scan_receiver {
            if let Ok(msg) = rx.try_recv() {
                self.is_scanning = false;
                self.scan_progress = None;
                self.scan_receiver = None;

                match msg {
                    ScanMessage::Finished(Ok(tree)) => {
                        self.dir_tree = Some(Arc::new(tree));
                        self.tree_view_state.reset(); // Reset view for new data
                    }
                    ScanMessage::Finished(Err(e)) => {
                        log::error!("Scan failed: {}", e);
                        // TODO: Show error dialog
                    }
                }
            }
        }
    }

    fn move_selected_to_trash(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.tree_view_state.selected_path.clone() {
            match trash::delete(&path) {
                Ok(_) => {
                    log::info!("Moved to trash: {:?}", path);
                    self.tree_view_state.selected_path = None;
                    
                    if let Some(tree_arc) = &mut self.dir_tree {
                        let tree = Arc::make_mut(tree_arc);
                        if tree.remove_node(&path).is_some() {
                            self.tree_view_state.treemap_cache = None;
                            
                            // Reset zoom if we deleted something we were zoomed into
                            if let Some(zoomed) = &self.tree_view_state.double_clicked_path {
                                if zoomed.starts_with(&path) {
                                    self.tree_view_state.double_clicked_path = None;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to trash {:?}: {:?}", path, e);
                }
            }
        }
    }

    fn force_delete_selected(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.tree_view_state.selected_path.clone() {
            #[cfg(unix)]
            let is_root = sudo::check() == sudo::RunningAs::Root;
            #[cfg(windows)]
            let is_root = false;
            
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };

            let handle_success = |app: &mut Self| {
                log::info!("Permanently deleted: {:?}", path);
                app.tree_view_state.selected_path = None;
                
                if let Some(tree_arc) = &mut app.dir_tree {
                    let tree = Arc::make_mut(tree_arc);
                    if tree.remove_node(&path).is_some() {
                        app.tree_view_state.treemap_cache = None;
                        
                        if let Some(zoomed) = &app.tree_view_state.double_clicked_path {
                            if zoomed.starts_with(&path) {
                                app.tree_view_state.double_clicked_path = None;
                            }
                        }
                    }
                }
            };

            match result {
                Ok(_) => {
                    handle_success(self);
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !is_root => {
                    #[cfg(unix)]
                    {
                        log::info!("Permission denied, attempting to elevate for deletion...");
                        let mut cmd = std::process::Command::new("pkexec");
                        cmd.arg("rm").arg("-rf").arg(&path);
                        
                        match cmd.status() {
                            Ok(status) if status.success() => {
                                handle_success(self);
                            }
                            _ => log::error!("Failed to elevate and delete data."),
                        }
                    }
                    #[cfg(windows)]
                    {
                        log::info!("Permission denied, attempting to elevate for deletion...");
                        let mut cmd = std::process::Command::new("powershell");
                        let arg_list = if path.is_dir() {
                            format!("/c rmdir /s /q \\\"{}\\\"", path.display())
                        } else {
                            format!("/c del /f /q \\\"{}\\\"", path.display())
                        };
                        cmd.arg("-NoProfile")
                           .arg("-Command")
                           .arg(format!("try {{ Start-Process cmd -ArgumentList '{}' -Verb RunAs -WindowStyle Hidden -Wait; exit 0 }} catch {{ exit 1 }}", arg_list));
                        
                        match cmd.status() {
                            Ok(status) if status.success() => {
                                // Verify it actually deleted, because UAC might have been cancelled
                                if !path.exists() {
                                    handle_success(self);
                                }
                            }
                            _ => log::error!("Failed to elevate and delete data."),
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to delete {:?}: {:?}", path, e);
                }
            }
        }
    }
}

impl eframe::App for RDirStatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Process background events
        self.update_scan_status();

        // 2. Request repaints if scanning to update progress bar smoothly
        if self.is_scanning {
            ctx.request_repaint();
        }

        // 3. Top Toolbar
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("📂 Open Directory...").clicked() {
                        ui.close_menu();
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.start_scan(path, ctx.clone());
                        }
                    }
                    ui.separator();
                    if ui.button("❌ Exit").clicked() {
                        ui.close_menu();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    ui.set_min_width(220.0); // Ensure the longer text fits

                    let has_selection = self.tree_view_state.selected_path.is_some();
                    
                    let is_dir = self.tree_view_state.selected_path.as_ref().map_or(false, |p| p.is_dir());
                    let type_str = if is_dir { "folder" } else { "file" };
                    let trash_name = if cfg!(windows) { "Recycle Bin" } else { "trash" };

                    if ui.add_enabled(has_selection, egui::Button::new(format!("🗑 Move (1) {} to {}", type_str, trash_name))).clicked() {
                        ui.close_menu();
                        self.move_selected_to_trash(ctx);
                    }

                    if ui.add_enabled(has_selection, egui::Button::new(format!("⚠️ Delete (1) {} Instantly", type_str))).clicked() {
                        ui.close_menu();
                        self.force_delete_selected(ctx);
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.tree_view_state.show_filters_panel, "Treemap Filters").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Zoom to Root (Reset Treemap Zoom)").clicked() {
                        self.tree_view_state.double_clicked_path = None;
                        ui.close_menu();
                    }
                });

                ui.separator();

                let path_str = self
                    .current_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "No directory selected".to_string());

                ui.label(path_str);
            });
        });

        // 4. Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.is_scanning {
                    ui.spinner();
                    if let Some(progress) = &self.scan_progress {
                        let files = progress.files_scanned.load(std::sync::atomic::Ordering::Relaxed);
                        let dirs = progress.dirs_scanned.load(std::sync::atomic::Ordering::Relaxed);
                        let path = progress.current_path.lock().unwrap().clone();
                        
                        let display_path = if path.len() > 64 {
                            format!("...{}", &path[path.len() - 61..])
                        } else {
                            path
                        };
                        
                        ui.label(format!("Scanning: {} files, {} dirs | {}", files, dirs, display_path));
                    }
                } else if let Some(tree) = &self.dir_tree {
                    ui.label(format!(
                        "Ready. Scanned {} files and {} directories in {:.2}s. Total size: {}",
                        tree.stats.total_files,
                        tree.stats.total_dirs,
                        tree.scan_duration.as_secs_f64(),
                        crate::human_readable_size(tree.total_size())
                    ));
                } else {
                    ui.label("Ready.");
                }
            });
        });

        // 4. Filters Side Panel
        if self.tree_view_state.show_filters_panel {
            egui::SidePanel::right("filters_panel")
                .resizable(true)
                .min_width(200.0)
                .show(ctx, |ui| {
                    ui.heading("Treemap Filters");
                    ui.separator();
                    ui.label("Filter by extension (e.g. 'rs, toml'):");
                    
                    let response = ui.text_edit_singleline(&mut self.tree_view_state.extension_filter);
                    if response.changed() {
                        self.tree_view_state.treemap_cache = None;
                        ctx.request_repaint();
                    }
                });
        }

        // 5. Main Content Area (Split between Tree View and Treemap)
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tree) = &self.dir_tree {
                egui::SidePanel::left("tree_view_panel")
                    .resizable(true)
                    .min_width(300.0)
                    .default_width(ui.available_width() * 0.4)
                    .show_inside(ui, |ui| {
                        crate::tree_view::ui(ui, tree, &mut self.tree_view_state);
                    });
                
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    crate::treemap::ui(ui, tree, &mut self.tree_view_state);
                });
            } else if !self.is_scanning {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 2.0 - 40.0);
                    let btn = egui::Button::new(
                        egui::RichText::new("📂 Choose Path to Start")
                            .size(24.0)
                            .strong(),
                    );
                    if ui.add_sized([400.0, 80.0], btn).clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.start_scan(path, ctx.clone());
                        }
                    }
                });
            }
        });
    }
}
