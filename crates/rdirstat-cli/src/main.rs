// RDirStat - CLI frontend
// License: GPL-2.0
//
// A fast disk usage analyzer for Linux.
// Combines the visual richness of QDirStat with WizTree-class scanning speed.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};

use rdirstat_core::file_info::{human_readable_size, FileInfo};
use rdirstat_core::stats::FileTypeStats;
use rdirstat_scan::{ScanOptions, Scanner};

#[derive(Parser)]
#[command(
    name = "rdirstat",
    about = "⚡ RDirStat — Fast Linux disk usage analyzer",
    long_about = "A fast, parallel disk usage analyzer for Linux.\nCombines the scanning speed of WizTree with the visual richness of QDirStat.",
    version
)]
struct Cli {
    /// Directory to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Maximum depth to scan (unlimited if not set)
    #[arg(short, long)]
    depth: Option<u32>,

    /// Number of top entries to show
    #[arg(short = 'n', long, default_value = "20")]
    top: usize,

    /// Follow symbolic links
    #[arg(short = 'L', long)]
    follow_links: bool,

    /// Cross filesystem boundaries
    #[arg(short = 'x', long)]
    cross_filesystems: bool,

    /// Skip hidden files and directories
    #[arg(short = 'H', long)]
    skip_hidden: bool,

    /// Patterns to exclude (can be specified multiple times)
    #[arg(short, long)]
    exclude: Vec<String>,

    /// Number of threads (0 = auto-detect)
    #[arg(short = 'j', long, default_value = "0")]
    threads: usize,

    /// Output format
    #[arg(short, long, value_enum, default_value = "tree")]
    format: OutputFormat,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the N largest files
    #[command(name = "top-files")]
    TopFiles {
        /// Number of files to show
        #[arg(short = 'n', long, default_value = "20")]
        count: usize,
    },

    /// Show the N largest directories
    #[command(name = "top-dirs")]
    TopDirs {
        /// Number of directories to show
        #[arg(short = 'n', long, default_value = "20")]
        count: usize,
    },

    /// Show file type breakdown by extension
    #[command(name = "types")]
    FileTypes {
        /// Number of extensions to show
        #[arg(short = 'n', long, default_value = "30")]
        count: usize,
    },
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// Tree view (like dust)
    Tree,
    /// JSON output
    Json,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();

    let options = ScanOptions {
        path: cli.path.clone(),
        follow_symlinks: cli.follow_links,
        cross_filesystems: cli.cross_filesystems,
        exclude_patterns: cli.exclude.clone(),
        max_depth: cli.depth,
        num_threads: cli.threads,
        skip_hidden: cli.skip_hidden,
        ..Default::default()
    };

    let scanner = Scanner::new(options);
    let progress = scanner.progress();

    // Start progress bar in a separate thread
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} Scanning... {msg} [{elapsed_precise}]"
        )
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );

    let pb_clone = pb.clone();
    let progress_clone = progress.clone();
    let progress_thread = thread::spawn(move || {
        loop {
            let files = progress_clone
                .files_scanned
                .load(std::sync::atomic::Ordering::Relaxed);
            let dirs = progress_clone
                .dirs_scanned
                .load(std::sync::atomic::Ordering::Relaxed);
            let bytes = progress_clone
                .bytes_counted
                .load(std::sync::atomic::Ordering::Relaxed);
            let path = progress_clone
                .current_path
                .lock()
                .map(|p| p.clone())
                .unwrap_or_default();

            // Truncate path for display
            let display_path = if path.len() > 50 {
                format!("...{}", &path[path.len() - 47..])
            } else {
                path
            };

            pb_clone.set_message(format!(
                "{} files, {} dirs, {} | {}",
                files,
                dirs,
                human_readable_size(bytes),
                display_path
            ));
            pb_clone.tick();

            thread::sleep(Duration::from_millis(80));

            // Check if we should stop (files+dirs will stop changing once scan is done)
            // We rely on the main thread finishing and joining us
            if pb_clone.is_finished() {
                break;
            }
        }
    });

    // Run the scan
    let tree = scanner.scan()?;
    pb.finish_and_clear();
    // Wait for progress thread to notice
    let _ = progress_thread.join();

    // Print scan summary
    eprintln!(
        "⚡ Scanned {} in {:.2}s — {} files, {} dirs, {}",
        tree.scan_path.display(),
        tree.scan_duration.as_secs_f64(),
        tree.stats.total_files,
        tree.stats.total_dirs,
        human_readable_size(tree.total_size()),
    );

    if tree.stats.hard_link_duplicates > 0 {
        eprintln!(
            "   {} hard link duplicates detected",
            tree.stats.hard_link_duplicates
        );
    }

    if tree.stats.total_errors > 0 {
        eprintln!(
            "   {} errors during scan",
            tree.stats.total_errors
        );
    }

    eprintln!();

    // Handle subcommands or default output
    match cli.command {
        Some(Commands::TopFiles { count }) => {
            print_top_files(&tree.root, count);
        }
        Some(Commands::TopDirs { count }) => {
            print_top_dirs(&tree.root, count);
        }
        Some(Commands::FileTypes { count }) => {
            print_file_types(&tree.root, count);
        }
        None => match cli.format {
            OutputFormat::Json => {
                println!("{}", tree.to_json()?);
            }
            OutputFormat::Tree => {
                print_tree(&tree.root, cli.top, tree.total_size());
            }
        },
    }

    Ok(())
}

/// Print a tree view of the largest entries (like dust)
fn print_tree(root: &FileInfo, max_entries: usize, total_size: u64) {
    println!(
        "{}  {} {}",
        format_bar(root.total_size, total_size, 30),
        pad_size(root.total_size),
        root.path.display()
    );

    let mut shown = 0;
    print_tree_recursive(root, max_entries, total_size, "", &mut shown, 0);
}

fn print_tree_recursive(
    node: &FileInfo,
    max_entries: usize,
    total_size: u64,
    prefix: &str,
    shown: &mut usize,
    depth: usize,
) {
    if depth > 5 || *shown >= max_entries {
        return;
    }

    let children = &node.children;
    let len = children.len().min(max_entries - *shown);

    for (i, child) in children.iter().take(len).enumerate() {
        let is_last = i == len - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        println!(
            "{}  {} {}{}",
            format_bar(child.total_size, total_size, 30),
            pad_size(child.total_size),
            prefix,
            // connector,
            format!("{}{}", connector, child.name),
        );

        *shown += 1;

        if child.is_dir() && *shown < max_entries {
            print_tree_recursive(
                child,
                max_entries,
                total_size,
                &format!("{}{}", prefix, child_prefix),
                shown,
                depth + 1,
            );
        }
    }
}

/// Print top-N largest files
fn print_top_files(root: &FileInfo, count: usize) {
    let files = root.top_n_files(count);
    println!("Top {} largest files:", files.len());
    println!("{:>12}  {}", "Size", "Path");
    println!("{:>12}  {}", "────", "────");

    for f in &files {
        println!("{:>12}  {}", human_readable_size(f.size), f.path.display());
    }
}

/// Print top-N largest directories
fn print_top_dirs(root: &FileInfo, count: usize) {
    let dirs = root.top_n_dirs(count);
    println!("Top {} largest directories:", dirs.len());
    println!("{:>12}  {:>8}  {:>6}  {}", "Size", "Files", "Dirs", "Path");
    println!(
        "{:>12}  {:>8}  {:>6}  {}",
        "────", "─────", "────", "────"
    );

    for d in &dirs {
        println!(
            "{:>12}  {:>8}  {:>6}  {}",
            human_readable_size(d.total_size),
            d.total_files,
            d.total_dirs,
            d.path.display()
        );
    }
}

/// Print file type breakdown
fn print_file_types(root: &FileInfo, count: usize) {
    let stats = FileTypeStats::from_tree(root);

    println!("File type breakdown:");
    println!(
        "{:>12}  {:>8}  {:>6}  {}",
        "Size", "Files", "%", "Extension"
    );
    println!(
        "{:>12}  {:>8}  {:>6}  {}",
        "────", "─────", "──", "─────────"
    );

    for ext in stats.by_extension.iter().take(count) {
        println!(
            "{:>12}  {:>8}  {:>5.1}%  .{}",
            human_readable_size(ext.total_size),
            ext.file_count,
            ext.percent,
            ext.extension
        );
    }

    println!();
    println!("By category:");
    println!("{:>12}  {:>8}  {:>6}  {}", "Size", "Files", "%", "Category");
    println!(
        "{:>12}  {:>8}  {:>6}  {}",
        "────", "─────", "──", "────────"
    );

    for cat in &stats.by_category {
        println!(
            "{:>12}  {:>8}  {:>5.1}%  {}",
            human_readable_size(cat.total_size),
            cat.file_count,
            cat.percent,
            cat.category.label()
        );
    }
}

/// Create a proportional bar visualization
fn format_bar(size: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return " ".repeat(width);
    }
    let ratio = size as f64 / total as f64;
    let filled = (ratio * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    format!(
        "\x1b[36m{}\x1b[90m{}\x1b[0m",
        "█".repeat(filled),
        "░".repeat(empty)
    )
}

/// Pad a size string to fixed width
fn pad_size(bytes: u64) -> String {
    format!("{:>10}", human_readable_size(bytes))
}
