// RDirStat - GUI frontend
// License: GPL-2.0
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{atomic::Ordering, Arc};
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;

use rdirstat_core::dir_tree::DirTree;
use rdirstat_core::file_info::human_readable_size;
use rdirstat_scan::{ScanOptions, ScanProgress, Scanner};

mod app;
mod treemap;
mod tree_view;
mod color_map;

use app::RDirStatApp;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "RDirStat",
        options,
        Box::new(|cc| Box::new(RDirStatApp::new(cc))),
    )?;

    Ok(())
}
