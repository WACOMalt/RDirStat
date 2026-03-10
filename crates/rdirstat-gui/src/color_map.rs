// RDirStat - Color Mapping for UI
// License: GPL-2.0

use eframe::egui::Color32;
use rdirstat_core::file_type::FileCategory;

pub fn category_color(category: &FileCategory, extension: &str) -> Color32 {
    match category {
        FileCategory::Image => Color32::from_rgb(255, 128, 0),        // Orange
        FileCategory::Video => Color32::from_rgb(200, 0, 200),        // Purple
        FileCategory::Audio => Color32::from_rgb(0, 200, 255),        // Light Blue
        FileCategory::Document => Color32::from_rgb(0, 255, 128),     // Green
        FileCategory::Archive => Color32::from_rgb(255, 50, 50),      // Red
        FileCategory::SourceCode => Color32::from_rgb(200, 200, 50),  // Yellow
        FileCategory::Executable => Color32::from_rgb(50, 200, 50),   // Dark Green
        FileCategory::Database => Color32::from_rgb(100, 100, 255),   // Blue
        FileCategory::Font => Color32::from_rgb(150, 150, 150),       // Gray
        FileCategory::Config => Color32::from_rgb(255, 150, 200),     // Pink
        FileCategory::Temporary => Color32::from_rgb(100, 50, 50),    // Brown
        FileCategory::Other => {
            if extension.is_empty() {
                Color32::from_rgb(100, 100, 100)
            } else {
                // Generate a deterministic color based on the extension string
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(extension, &mut hasher);
                let hash = std::hash::Hasher::finish(&hasher);
                
                let r = ((hash >> 16) & 0xFF) as u8;
                let g = ((hash >> 8) & 0xFF) as u8;
                let b = (hash & 0xFF) as u8;
                
                // Keep the brightness in a pleasant medium-pastel range so it's not pure black or white
                let normalize = |c: u8| 80 + (c as u32 * 100 / 255) as u8;
                Color32::from_rgb(normalize(r), normalize(g), normalize(b))
            }
        }
    }
}
