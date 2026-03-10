# RDirStat — Linux Port Plan

## Vision

**RDirStat** is a fast, modern Linux disk usage analyzer that combines the **visual richness of QDirStat** (treemap, tree view, file type breakdown) with the **scanning speed of WizTree**. Written in **Rust** with a native GUI toolkit.

---

## Why a New Project?

| Tool | Strengths | Weaknesses |
|------|-----------|------------|
| **QDirStat** (C++/Qt) | Rich GUI, treemap, file type stats, exclude rules, package awareness | Single-threaded scanning, slow on large filesystems, Qt/C++ maintenance burden |
| **dust** (Rust) | Fast parallel scanning via `rayon`, CLI, good Rust idioms | CLI-only, no GUI, no treemap, no file type view |
| **WizTree** (Windows) | Blazing fast via NTFS MFT direct read, great UI with treemap + file type view + top-N files | Windows-only, closed source, MFT trick is NTFS-specific |

**RDirStat** aims to take the best of all three.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────┐
│                  RDirStat GUI                    │
│  ┌───────────┐ ┌───────────┐ ┌────────────────┐ │
│  │ Tree View │ │ File Type │ │  Treemap View  │ │
│  │ (sortable │ │   Stats   │ │  (squarified)  │ │
│  │  columns) │ │ (by ext)  │ │                │ │
│  └─────┬─────┘ └─────┬─────┘ └───────┬────────┘ │
│        └──────────────┴───────────────┘          │
│                       │                          │
│              Shared DirTree Model                │
│                       │                          │
│        ┌──────────────┴───────────────┐          │
│        │      Scanner Engine          │          │
│        │  (Rust, parallel, io_uring)  │          │
│        └──────────────────────────────┘          │
└──────────────────────────────────────────────────┘
```

### Two-Binary Architecture

1. **`rdirstat-scan`** — Standalone Rust CLI scanner (no GUI dependencies)
   - Can be used independently like `dust`
   - Outputs structured data (JSON/MessagePack) for the GUI
   - Can also print a human-readable tree (like `dust` does)
   
2. **`rdirstat`** — GUI application
   - Embeds and invokes the scanner
   - Renders treemap, tree view, file type stats
   - GUI framework: **egui** (pure Rust, GPU-accelerated, cross-platform) or **Slint** (QML-like declarative Rust GUI)

---

## Speed Strategy — How to Match WizTree on Linux

### Background: Why WizTree Is Fast

WizTree reads the NTFS **Master File Table (MFT)** directly — a single contiguous data structure that indexes every file on the volume. This bypasses the normal file-by-file `FindFirstFile`/`FindNextFile` API entirely. On Linux, there is **no universal equivalent** of the MFT, but there are several techniques that can achieve comparable speed:

### Tier 1: Parallel Directory Walking (Baseline — ~5-10× faster than QDirStat)

- Use **`rayon`** for work-stealing parallelism across directory entries
- Use **`jwalk`** crate (built on rayon) which is ~4× faster than `walkdir`
- QDirStat is **entirely single-threaded** — just going parallel is a huge win

### Tier 2: Syscall Optimization (~2-5× on top of Tier 1)

- **`getdents64`** directly instead of `readdir()` libc wrapper — avoids per-entry overhead
- **`fstatat()`** with `AT_SYMLINK_NOFOLLOW | AT_NO_AUTOMOUNT` (QDirStat already does this) 
- **`statx()`** (Linux 4.11+) — only request the fields we need (size, mtime, mode) using the mask, avoids fetching unnecessary metadata
- **Inode-order traversal** — sort directory entries by inode number before `stat`-ing them. This minimizes disk seeks on rotational drives. QDirStat already does this; we must preserve it.

### Tier 3: io_uring Acceleration (~2-10× on top of Tier 2, especially on NVMe)

- Batch **`statx`** calls via **`io_uring`** submission ring — submit hundreds of stat requests in a single syscall
- Batch **`getdents64`** via `io_uring` (Linux 6.x+) 
- Use `SQPOLL` mode for near-zero syscall overhead on supported kernels
- **Graceful fallback**: detect kernel version at runtime, fall back to sync syscalls on older kernels

### Tier 4: Filesystem-Specific Fast Paths (Advanced, Future)

- **ext4**: Parse the inode table directly via `debugfs`-style raw block reads (read-only, requires root). This is the closest Linux equivalent to WizTree's MFT reading.
- **btrfs**: Use `btrfs inspect-internal` / `BTRFS_IOC_TREE_SEARCH` ioctl to walk the filesystem tree natively
- **XFS**: Use `xfs_db` / `bulkstat` ioctl (`XFS_IOC_FSBULKSTAT`) — XFS has a dedicated bulk-stat interface purpose-built for tools like this
- These are optional accelerators — the tool should always work via standard POSIX APIs

### Tier 5: Caching and Incremental Scanning (Future)

- Cache scan results to disk (like QDirStat's `.qdirstat.cache.gz`)
- Use `fanotify` / `inotify` to detect changes since last scan
- Only re-scan changed subtrees

### Expected Performance Targets

| Scenario | QDirStat | RDirStat Target |
|----------|----------|-----------------|
| 1M files, SSD | ~45-90s | ~2-5s |
| 1M files, HDD | ~120-300s | ~15-30s |
| 10M files, NVMe | untested/very slow | ~10-20s |

---

## Feature Set — Phase by Phase

### Phase 1: Core Scanner + CLI (MVP)

- [ ] Parallel recursive directory walker using `jwalk` + `rayon`
- [ ] `statx()` for metadata, with `fstatat()` fallback
- [ ] Inode-order traversal within each directory
- [ ] Filesystem boundary detection (don't cross mount points by default)
- [ ] Hard link detection and deduplication
- [ ] JSON output of the full directory tree with sizes
- [ ] CLI mode: print top-N largest files/dirs (like `dust`)
- [ ] Progress reporting (files scanned, bytes counted, elapsed time)
- [ ] Symlink handling (follow/don't-follow option)
- [ ] Exclude patterns (glob-based, like `.git`, `node_modules`)

### Phase 2: GUI — Tree View + Treemap

- [ ] GUI framework setup (egui or Slint — decision needed)
- [ ] Directory tree view with sortable columns:
  - Name, Size (bytes + human-readable), % of parent, Files count, Dirs count, Last Modified
- [ ] Squarified treemap visualization (color-coded by file extension)
- [ ] Selection syncing between tree view and treemap
- [ ] Breadcrumb navigation bar
- [ ] Zoom into/out of subtrees in treemap
- [ ] Context menu: Open in file manager, Open terminal here, Delete, Properties

### Phase 3: WizTree Feature Parity

- [ ] **File Type View**: table grouped by extension — total size, count, % of total
- [ ] **Top N Largest Files** view (configurable N, default 1000)
- [ ] **Top N Largest Folders** view  
- [ ] **Top N Newest/Oldest Files** view
- [ ] **Date filters**: show files modified before/after a given date
- [ ] **Search/Filter bar**: filter by name, extension, size range, date range
- [ ] **Export**: CSV/JSON export of any view

### Phase 4: Performance — io_uring + Fast Paths

- [ ] `io_uring` batched `statx()` scanning backend
- [ ] `io_uring` batched `getdents64()` on supported kernels
- [ ] Runtime kernel version detection and automatic backend selection
- [ ] XFS `bulkstat` ioctl fast path
- [ ] btrfs `TREE_SEARCH` ioctl fast path
- [ ] Benchmark suite comparing all backends

### Phase 5: Polish + QDirStat Feature Parity

- [ ] Scan cache files (save/load scan snapshots)
- [ ] Package manager awareness (show which package owns a file — dpkg, rpm, pacman)
- [ ] Exclude rules (regex, glob, directory-level, file-child-level)
- [ ] File age statistics (histogram of file modification times)
- [ ] File size statistics (percentile distribution)
- [ ] Unpackaged files view
- [ ] Cleanup actions (configurable shell commands per file type)
- [ ] Settings/preferences dialog
- [ ] Dark mode / theme support
- [ ] Localization/i18n

---

## Technology Decisions

### Language: Rust

- Memory safety without GC overhead
- Excellent parallelism (`rayon`, `crossbeam`)
- Great async I/O (`io_uring` via `io-uring` crate)
- Mature ecosystem for filesystem work (`jwalk`, `walkdir`, `nix`, `libc`)
- Can produce both CLI tools and GUI apps

### GUI Framework: Decision Required

| Option | Pros | Cons |
|--------|------|------|
| **egui** | Pure Rust, GPU-accelerated, immediate-mode, easy to embed | Immediate-mode can be less efficient for static layouts, less native look |
| **Slint** | Declarative QML-like syntax, native look, Rust backend | Newer, smaller community, licensing considerations |
| **iced** | Elm-like architecture, pure Rust, good for complex UIs | Treemap rendering may need custom widget |
| **gtk4-rs** | Mature, native Linux look, strong ecosystem | C library FFI, heavier dependency |

**Recommendation**: Start with **egui** for fastest iteration. It's pure Rust, has excellent custom rendering support (critical for treemap), and is well-suited for data-heavy applications. If native look-and-feel becomes important, migrate to **gtk4-rs** or **Slint** later.

### Key Rust Crates

| Crate | Purpose |
|-------|---------|
| `jwalk` | Parallel directory walking |
| `rayon` | Data parallelism |
| `io-uring` | Linux io_uring bindings |
| `nix` / `libc` | Low-level Linux syscalls (`statx`, `getdents64`) |
| `serde` + `serde_json` | Serialization for scan results |
| `clap` | CLI argument parsing |
| `indicatif` | Progress bars for CLI mode |
| `egui` + `eframe` | GUI framework |
| `crossbeam` | Lock-free concurrent data structures |

---

## Project Structure

```
RDirStat/
├── Cargo.toml              # Workspace manifest
├── crates/
│   ├── rdirstat-core/      # Core data model (DirTree, FileInfo, etc.)
│   │   └── src/
│   ├── rdirstat-scan/      # Scanner engine (parallel walker, io_uring, etc.)
│   │   └── src/
│   ├── rdirstat-cli/       # CLI frontend (like dust)
│   │   └── src/
│   └── rdirstat-gui/       # GUI frontend (egui)
│       └── src/
├── benches/                # Benchmarks
├── tests/                  # Integration tests
└── README.md
```

---

## Handoff Guidelines for Coding Agents

Each phase is designed to be self-contained enough for a coding agent to pick up:

1. **Phase 1 agent** needs strong Rust + systems programming knowledge. Focus on `crates/rdirstat-core` and `crates/rdirstat-scan` and `crates/rdirstat-cli`. Reference `dust/src/dir_walker.rs` for parallel walking patterns and `qdirstat/src/DirReadJob.cpp` for inode-order + fstatat patterns.

2. **Phase 2 agent** needs Rust + egui experience. Focus on `crates/rdirstat-gui`. The treemap is the hardest widget — reference `qdirstat/src/TreemapTile.cpp` and `TreemapView.cpp` for the squarified treemap algorithm.

3. **Phase 3 agent** can work on top of Phase 2 GUI, adding views and filters. Mostly UI work.

4. **Phase 4 agent** needs deep Linux kernel knowledge. Implements `io_uring` integration and filesystem-specific ioctls in `crates/rdirstat-scan`.

5. **Phase 5 agent** focuses on QDirStat feature parity. Reference `qdirstat/src/` extensively.

---

## Open Questions / Decisions Needed

1. **GUI framework**: egui vs Slint vs gtk4-rs? (Recommendation: egui)
2. **Treemap algorithm**: Squarified (like QDirStat) vs Cushion (like SequoiaView)? (Recommendation: squarified)
3. **License**: GPL v2 (to match QDirStat) vs MIT/Apache-2.0?
4. **io_uring minimum kernel version**: 5.6+ (for full feature set) vs 5.1+ (basic)?
5. **Should Phase 1 CLI be compatible with `dust` arguments?** Could position RDirStat as a `dust` replacement too.
