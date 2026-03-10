// RDirStat - Squarified Treemap implementation
// License: GPL-2.0

use std::sync::Arc;
use eframe::egui::{self, Color32};

use rdirstat_core::dir_tree::DirTree;
use rdirstat_core::file_info::FileInfo;

use crate::tree_view::{CachedNode, TreeViewState, TreemapCache};
use crate::color_map::category_color;

use rdirstat_core::file_type::FileCategorizer;

struct FilteredNode<'a> {
    info: &'a FileInfo,
    filtered_size: u64,
    children: Vec<FilteredNode<'a>>,
}

impl<'a> FilteredNode<'a> {
    fn build(node: &'a FileInfo, valid_exts: &std::collections::HashSet<String>) -> Option<Self> {
        let mut children = Vec::new();
        let mut filtered_size = 0;

        if node.is_dir() {
            for child in &node.children {
                if let Some(filtered_child) = Self::build(child, valid_exts) {
                    if !child.is_hardlink_duplicate {
                        filtered_size += filtered_child.filtered_size;
                    }
                    children.push(filtered_child);
                }
            }
            if filtered_size > 0 {
                Some(Self {
                    info: node,
                    filtered_size,
                    children,
                })
            } else {
                None
            }
        } else {
            // It's a file
            let ext = node.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if valid_exts.is_empty() || valid_exts.contains(&ext) {
                Some(Self {
                    info: node,
                    filtered_size: node.size,
                    children: Vec::new(),
                })
            } else {
                None
            }
        }
    }
}

pub fn ui(ui: &mut egui::Ui, tree: &Arc<DirTree>, state: &mut TreeViewState) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    
    if !ui.is_rect_visible(rect) {
        return;
    }

    // Right-click to go up a directory level
    if response.secondary_clicked() {
        if let Some(current_drilldown) = &state.double_clicked_path {
            if let Some(parent) = current_drilldown.parent() {
                // If it's the root itself, clear it
                if parent == tree.root.path {
                    state.double_clicked_path = None;
                } else {
                    state.double_clicked_path = Some(parent.to_path_buf());
                }
            } else {
                state.double_clicked_path = None;
            }
        }
    }

    let painter = ui.painter_at(rect);
    
    // Background
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 20, 20));

    let ft = FileCategorizer::new();

    // Determine the root node for the Treemap
    let mut treemap_root = &tree.root;
    if let Some(drill_path) = &state.double_clicked_path {
        if let Some(found) = find_node(&tree.root, drill_path) {
            treemap_root = found;
        }
    }

    // Breadcrumbs before the layout recursion, but inside the main view?
    // Wait, the treemap takes the whole rect via allocate_exact_size at the top.
    // It's cleaner to draw the breadcrumbs as an overlay like we did with the button,
    // but formatted horizontally with a slightly dark background so it's readable.
    if treemap_root.path != tree.root.path {
        let area_id = egui::Id::new("breadcrumb_overlay");
        egui::Area::new(area_id)
            .fixed_pos(rect.left_top() + egui::vec2(10.0, 10.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::window(&ui.ctx().style())
                    .fill(Color32::from_rgba_unmultiplied(20, 20, 20, 220)) // semi-transparent dark background
                    .inner_margin(egui::Margin::same(6.0))
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // The path relative to root
                            // example: /home/user/project (tree.root) -> target: src/gui
                            let mut current_path = tree.root.path.clone();

                            // Root breadcrumb
                            let root_name = current_path
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| current_path.to_string_lossy().to_string());
                            
                            if ui.link(root_name).clicked() {
                                state.double_clicked_path = None;
                            }

                            // If we are deeper, render the intermediate paths
                            if let Ok(dist) = treemap_root.path.strip_prefix(&tree.root.path) {
                                for comp in dist.components() {
                                    ui.label(">");
                                    current_path.push(comp);
                                    
                                    // Make sure we capture a copy of the path for the closure
                                    let click_path = current_path.clone();
                                    let name = comp.as_os_str().to_string_lossy();
                                    
                                    if ui.link(name).clicked() {
                                        if click_path == tree.root.path {
                                            state.double_clicked_path = None;
                                        } else {
                                            state.double_clicked_path = Some(click_path);
                                        }
                                    }
                                }
                            }
                        });
                    });
            });
    }

    // Squarified layout recursion (Cached)
    let needs_rebuild = state.treemap_cache.as_ref().map_or(true, |cache| {
        cache.rect.size() != rect.size() || cache.root_path != treemap_root.path
    });

    if needs_rebuild {
        let width = rect.width().max(1.0) as usize;
        let height = rect.height().max(1.0) as usize;
        
        let mut pixels = vec![egui::Color32::from_rgb(20, 20, 20); width * height];
        
        // Load a placeholder texture, we will immediately overwrite it
        let placeholder = egui::ColorImage::new([1, 1], egui::Color32::TRANSPARENT);
        let mut new_cache = TreemapCache {
            rect,
            root_path: treemap_root.path.clone(),
            nodes: Vec::new(),
            rect_map: std::collections::HashMap::new(),
            texture: ui.ctx().load_texture("treemap_cache", placeholder, egui::TextureOptions::NEAREST),
        };

        let valid_exts: std::collections::HashSet<String> = state.extension_filter
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        // Build the FilteredNode shadow tree
        let filtered_root = if let Some(root) = FilteredNode::build(treemap_root, &valid_exts) {
            root
        } else {
            FilteredNode {
                info: treemap_root,
                filtered_size: 0,
                children: Vec::new(),
            }
        };

        if filtered_root.filtered_size > 0 {
            layout_and_build_cache(
                &filtered_root,
                rect,
                &mut new_cache,
                &mut pixels,
                width,
                height,
                &ft,
            );
        }

        let image = egui::ColorImage {
            size: [width, height],
            pixels,
        };
        new_cache.texture = ui.ctx().load_texture("treemap_cache", image, egui::TextureOptions::NEAREST);

        state.treemap_cache = Some(new_cache);
    }

    // Render from cache
    if let Some(cache) = &state.treemap_cache {
        // Draw the cached texture background
        painter.image(
            cache.texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Process interactions in reverse order to prefer deeper nodes (drawn last/smallest)
        if let Some(pos) = response.hover_pos() {
            if rect.contains(pos) {
                // Find highest matching node
                for node in cache.nodes.iter().rev() {
                    if node.rect.contains(pos) {
                        if response.clicked() {
                            state.selected_path = Some(node.path.clone());
                        }

                        if response.double_clicked() {
                            if node.is_dir {
                                state.double_clicked_path = Some(node.path.clone());
                            } else if let Some(parent) = node.path.parent() {
                                state.double_clicked_path = Some(parent.to_path_buf());
                            }
                        }

                        // Apply subtle highlight
                        painter.rect_stroke(node.rect, 0.0, egui::Stroke::new(1.0, egui::Color32::WHITE));

                        // Tooltip
                        egui::show_tooltip_at_pointer(ui.ctx(), egui::Id::new("treemap_tooltip"), |ui| {
                            ui.label(&node.name);
                            ui.label(rdirstat_core::file_info::human_readable_size(node.size));
                            if !node.is_dir {
                                ui.label(node.path.display().to_string());
                            }
                        });

                        break; // Stop checking hover target after first hit in reverse order
                    }
                }
            }
        }

        // Apply selection box overlay
        if let Some(selected) = &state.selected_path {
            if let Some(&selected_rect) = cache.rect_map.get(selected) {
                painter.rect_stroke(selected_rect, 0.0, egui::Stroke::new(2.0, egui::Color32::YELLOW));
            }
        }
    }
}

fn find_node<'a>(node: &'a FileInfo, target_path: &std::path::Path) -> Option<&'a FileInfo> {
    if node.path == target_path {
        return Some(node);
    }
    
    if target_path.starts_with(&node.path) {
        for child in &node.children {
            if let Some(found) = find_node(child, target_path) {
                return Some(found);
            }
        }
    }
    None
}

fn layout_and_build_cache(
    node: &FilteredNode,
    rect: egui::Rect,
    cache: &mut TreemapCache,
    pixels: &mut [egui::Color32],
    width: usize,
    height: usize,
    ft: &FileCategorizer,
) {
    // Always add this node to the lookup map
    cache.rect_map.insert(node.info.path.clone(), rect);

    if rect.width() < 1.0 || rect.height() < 1.0 {
        let ext = node.info.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let cat = ft.categorize(&ext.to_lowercase());
        let color = category_color(&cat, ext);

        cache.nodes.push(CachedNode {
            rect,
            path: node.info.path.clone(),
            is_dir: node.info.is_dir(),
            name: node.info.name.clone(),
            size: node.filtered_size,
        });
        
        fill_rect(pixels, width, height, rect, cache.rect, color);
        return;
    }

    if node.info.is_file() || node.children.is_empty() {
        let ext = node.info.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let cat = ft.categorize(&ext.to_lowercase());
        let color = category_color(&cat, ext);

        cache.nodes.push(CachedNode {
            rect,
            path: node.info.path.clone(),
            is_dir: node.info.is_dir(),
            name: node.info.name.clone(),
            size: node.filtered_size,
        });

        fill_rect(pixels, width, height, rect, cache.rect, color);
    } else {
        cache.nodes.push(CachedNode {
            rect,
            path: node.info.path.clone(),
            is_dir: node.info.is_dir(),
            name: node.info.name.clone(),
            size: node.filtered_size,
        });

        // Directory node - layout children
        // Calculate the total size of children to use as the denominator for layout.
        // This avoids empty space caused by directory metadata sizes (self.size),
        // and safely ignores hardlink duplicates whose size wasn't added to the parent's total.
        let layout_total = node.children.iter()
            .map(|c| if c.info.is_hardlink_duplicate { 0 } else { c.filtered_size })
            .sum::<u64>() as f32;

        if layout_total <= 0.0 {
            return;
        }

        // Do not add any margins or borders to directory boxes to prevent small files
        // from being consumed by black lines.
        let mut inner_rect = rect;
        
        // Simple slice-and-dice layout for now (Squarified is complex, let's start with slice-and-dice alternating)
        // Sort children by size descending
        let mut children: Vec<&FilteredNode> = node.children.iter().collect();
        children.sort_by(|a, b| b.filtered_size.cmp(&a.filtered_size));

        // Squarified layout algorithm (Bruls et al.)
        let mut remaining_total = layout_total;
        let mut current_rect = inner_rect;
        let mut idx = 0;

        while idx < children.len() {
            let mut row_size = 0.0;
            let mut best_worst_ratio = f32::INFINITY;
            let mut row_count = 0;

            let r_area = current_rect.width() * current_rect.height();
            let shortest_edge = current_rect.width().min(current_rect.height());

            for j in idx..children.len() {
                let child_size = if children[j].info.is_hardlink_duplicate { 0.0 } else { children[j].filtered_size as f32 };
                if child_size == 0.0 {
                    row_count += 1;
                    continue;
                }

                let new_row_size = row_size + child_size;
                let new_area = r_area * (new_row_size / remaining_total);
                let thickness = new_area / shortest_edge;

                // Calculate worst aspect ratio in the proposed row
                let mut worst_ratio: f32 = 0.0;
                for k in idx..=j {
                    let cs = if children[k].info.is_hardlink_duplicate { 0.0 } else { children[k].filtered_size as f32 };
                    if cs > 0.0 {
                        let length = shortest_edge * (cs / new_row_size);
                        let ratio = if thickness > length { thickness / length } else { length / thickness };
                        if ratio > worst_ratio {
                            worst_ratio = ratio;
                        }
                    }
                }

                if worst_ratio <= best_worst_ratio || row_size == 0.0 {
                    best_worst_ratio = worst_ratio;
                    row_size = new_row_size;
                    row_count += 1;
                } else {
                    break;
                }
            }

            if row_count == 0 {
                row_count = 1; // Fallback to prevent infinite loop
                let child_size = if children[idx].info.is_hardlink_duplicate { 0.0 } else { children[idx].filtered_size as f32 };
                row_size = child_size;
            }

            // Now lay out row_count items
            let is_horizontal = current_rect.width() > current_rect.height();
            let row_ratio = if remaining_total > 0.0 { row_size / remaining_total } else { 0.0 };
            
            let thickness = if is_horizontal { current_rect.width() * row_ratio } else { current_rect.height() * row_ratio };
            
            let mut row_rect = current_rect;
            if is_horizontal {
                row_rect.set_width(thickness);
                current_rect.min.x += thickness;
            } else {
                row_rect.set_height(thickness);
                current_rect.min.y += thickness;
            }

            // Lay out items within row_rect
            let mut pos = if is_horizontal { row_rect.min.y } else { row_rect.min.x };
            for j in idx..(idx + row_count) {
                let child = children[j];
                let child_size = if child.info.is_hardlink_duplicate { 0.0 } else { child.filtered_size as f32 };
                if child_size > 0.0 {
                    let child_ratio = child_size / row_size;
                    
                    let mut child_rect = row_rect;
                    if is_horizontal {
                        let length = row_rect.height() * child_ratio;
                        child_rect.min.y = pos;
                        child_rect.set_height(length);
                        pos += length;
                    } else {
                        let length = row_rect.width() * child_ratio;
                        child_rect.min.x = pos;
                        child_rect.set_width(length);
                        pos += length;
                    }
                    layout_and_build_cache(child, child_rect, cache, pixels, width, height, ft);
                }
            }

            idx += row_count;
            remaining_total -= row_size;
        }
    }
}

fn fill_rect(
    pixels: &mut [egui::Color32],
    width: usize,
    height: usize,
    rect: egui::Rect,
    base_rect: egui::Rect,
    color: egui::Color32,
) {
    let mut min_x = (rect.min.x - base_rect.min.x).round() as isize;
    let mut min_y = (rect.min.y - base_rect.min.y).round() as isize;
    let mut max_x = (rect.max.x - base_rect.min.x).round() as isize;
    let mut max_y = (rect.max.y - base_rect.min.y).round() as isize;

    min_x = min_x.clamp(0, width as isize);
    min_y = min_y.clamp(0, height as isize);
    max_x = max_x.clamp(0, width as isize);
    max_y = max_y.clamp(0, height as isize);

    let r = color.r() as f32;
    let g = color.g() as f32;
    let b = color.b() as f32;
    let a = color.a();
    
    let cx = (rect.max.x - rect.min.x) / 2.0;
    let cy = (rect.max.y - rect.min.y) / 2.0;
    
    // Max distance from center to a corner is sqrt(cx^2 + cy^2)
    let max_dist = (cx * cx + cy * cy).sqrt().max(1.0);

    for y in min_y..max_y {
        let y_offset = (y as usize) * width;
        let local_y = (y - min_y) as f32;
        
        let dy = local_y - cy;
        
        for x in min_x..max_x {
            let local_x = (x - min_x) as f32;
            let dx = local_x - cx;
            
            let dist = (dx * dx + dy * dy).sqrt();
            
            // Circular gradient: brightest (1.25x) at center, darker (0.85x) at edges
            let factor = 1.25 - (0.4 * (dist / max_dist));
            
            let current_color = egui::Color32::from_rgba_unmultiplied(
                (r * factor).clamp(0.0, 255.0) as u8,
                (g * factor).clamp(0.0, 255.0) as u8,
                (b * factor).clamp(0.0, 255.0) as u8,
                a,
            );

            pixels[y_offset + (x as usize)] = current_color;
        }
    }
}
