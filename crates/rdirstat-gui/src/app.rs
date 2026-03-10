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
            ui.horizontal(|ui| {
                if ui.button("📂 Open Directory").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.start_scan(path, ctx.clone());
                    }
                }

                ui.separator();

                let mut path_str = self
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
                ui.centered_and_justified(|ui| {
                    ui.heading("Click 'Open Directory' to start a scan.");
                });
            }
        });
    }
}
