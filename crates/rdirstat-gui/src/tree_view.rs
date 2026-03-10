// RDirStat - Tree View implementation
// License: GPL-2.0

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use rdirstat_core::dir_tree::DirTree;
use rdirstat_core::file_info::{human_readable_size, FileInfo};

pub struct CachedNode {
    pub rect: egui::Rect,
    pub path: PathBuf,
    pub is_dir: bool,
    pub name: String,
    pub size: u64,
}

pub struct TreemapCache {
    pub rect: egui::Rect,
    pub root_path: PathBuf,
    pub nodes: Vec<CachedNode>,
    pub rect_map: std::collections::HashMap<PathBuf, egui::Rect>,
    pub texture: egui::TextureHandle,
}

#[derive(Default)]
pub struct TreeViewState {
    pub expanded_paths: HashSet<PathBuf>,
    pub selected_path: Option<PathBuf>,
    pub double_clicked_path: Option<PathBuf>,
    pub treemap_cache: Option<TreemapCache>,
    pub show_filters_panel: bool,
    pub extension_filter: String,
}

impl TreeViewState {
    pub fn reset(&mut self) {
        self.expanded_paths.clear();
        self.selected_path = None;
        self.double_clicked_path = None;
        self.treemap_cache = None;
        self.extension_filter.clear();
    }
}

pub fn ui(ui: &mut egui::Ui, tree: &Arc<DirTree>, state: &mut TreeViewState) {
    let available_height = ui.available_height();

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(300.0).resizable(true).clip(true)) // Name
        .column(Column::initial(100.0).resizable(true)) // Size
        .column(Column::initial(100.0).resizable(true)) // Percent
        .column(Column::initial(60.0).resizable(true)) // Files
        .column(Column::initial(60.0).resizable(true)) // Dirs
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.strong("Name");
            });
            header.col(|ui| {
                ui.strong("Size");
            });
            header.col(|ui| {
                ui.strong("Percent");
            });
            header.col(|ui| {
                ui.strong("Files");
            });
            header.col(|ui| {
                ui.strong("Dirs");
            });
        })
        .body(|mut body| {
            let total_size = tree.total_size();
            
            // Start recursion from the root
            // Because TableBuilder requires pre-calculating the number of rows or using an iterative/recursive manual approach,
            // we will use the dynamic manual row adding approach.
            draw_node(&mut body, &tree.root, state, 0, total_size);
        });
}

fn draw_node(
    body: &mut egui_extras::TableBody,
    node: &FileInfo,
    state: &mut TreeViewState,
    depth: usize,
    total_tree_size: u64,
) {
    let is_dir = node.is_dir();
    let is_expanded = state.expanded_paths.contains(&node.path);

    body.row(20.0, |mut row| {
        row.col(|ui| {
            ui.horizontal(|ui| {
                // Indentation
                ui.add_space(depth as f32 * 16.0);

                if is_dir {
                    let icon = if is_expanded { "▼" } else { "▶" };
                    if ui.selectable_label(false, icon).clicked() {
                        if is_expanded {
                            state.expanded_paths.remove(&node.path);
                        } else {
                            state.expanded_paths.insert(node.path.clone());
                        }
                    }
                } else {
                    ui.add_space(12.0); // Align with arrows
                }

                // File/Dir Icon
                let icon = match node.kind {
                    rdirstat_core::file_info::FileKind::Directory => "📁",
                    rdirstat_core::file_info::FileKind::Symlink => "🔗",
                    _ => "📄",
                };

                let is_selected = state.selected_path.as_ref() == Some(&node.path);
                let resp = ui.selectable_label(is_selected, format!("{} {}", icon, node.name));
                if resp.clicked() {
                    state.selected_path = Some(node.path.clone());
                }
                if resp.double_clicked() && is_dir {
                    if state.double_clicked_path.as_ref() == Some(&node.path) {
                        // Already zoomed in on this, zoom out
                        if let Some(parent) = node.path.parent() {
                            state.double_clicked_path = Some(parent.to_path_buf());
                        } else {
                            state.double_clicked_path = None;
                        }
                    } else {
                        state.double_clicked_path = Some(node.path.clone());
                    }
                }
            });
        });

        row.col(|ui| {
            ui.label(human_readable_size(node.total_size));
        });

        row.col(|ui| {
            let pct = if total_tree_size > 0 {
                (node.total_size as f64 / total_tree_size as f64) * 100.0
            } else {
                0.0
            };
            
            // Draw a small inline progress bar for percent
            let (rect, _resp) = ui.allocate_exact_size(egui::vec2(80.0, 10.0), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                painter.rect_filled(rect, 2.0, egui::Color32::from_gray(60));
                
                let fill_width = (pct as f32 / 100.0) * rect.width();
                let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, rect.height()));
                painter.rect_filled(fill_rect, 2.0, egui::Color32::from_rgb(0, 150, 250));
            }
            
            ui.label(format!("{:.1}%", pct));
        });

        row.col(|ui| {
            if is_dir {
                ui.label(node.total_files.to_string());
            }
        });

        row.col(|ui| {
            if is_dir {
                ui.label(node.total_dirs.to_string());
            }
        });
    });

    if is_expanded && is_dir {
        for child in &node.children {
            draw_node(body, child, state, depth + 1, total_tree_size);
        }
    }
}
