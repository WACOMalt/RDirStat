# RDirStat — Linux Port TODO

## Phase 1: Core Scanner + CLI (MVP)

### Project Setup
- [x] Initialize Cargo workspace with `rdirstat-core`, `rdirstat-scan`, `rdirstat-cli` crates
- [x] Set up CI (GitHub Actions — build + test + clippy + fmt)
- [x] Add dependencies: `jwalk`, `rayon`, `libc`/`nix`, `serde`, `clap`, `indicatif`
- [x] Write README with project vision and build instructions

### Core Data Model (`rdirstat-core`)
- [x] `FileInfo` struct: path, name, size (bytes + blocks), mode, mtime, nlinks, inode, device
- [x] `DirInfo` struct: extends FileInfo with children vec, total size, file count, dir count
- [x] `DirTree` struct: root node, filesystem metadata, scan stats
- [x] File type categorization (by extension → MIME category mapping)
- [x] Serialization (serde) for JSON/MessagePack output

### Scanner Engine (`rdirstat-scan`)
- [x] Parallel directory walker using `jwalk` with rayon thread pool
- [x] `statx()` metadata calls with mask for only needed fields
- [x] Fallback to `fstatat()` on older kernels
- [x] Inode-order sorting within each directory before stat calls
- [x] Filesystem boundary detection via device ID comparison
- [x] Hard link tracking (inode+device set) to avoid double-counting
- [x] Symlink handling (follow / no-follow option)
- [x] Exclude patterns (glob-based: `.git`, `node_modules`, etc.)
- [x] Progress callback (files scanned, bytes counted, current path)
- [x] Error handling (permission denied, broken symlinks, etc.)

### CLI Frontend (`rdirstat-cli`)
- [x] Argument parsing with `clap`: target path, depth, top-N, output format
- [x] Tree display mode (like `dust`)
- [x] Top-N largest files mode
- [x] Top-N largest directories mode  
- [x] JSON output mode
- [x] Progress bar during scan
- [x] Human-readable size formatting

### Testing
- [x] Unit tests for `FileInfo`/`DirInfo` size aggregation
- [x] Unit tests for inode deduplication
- [x] Integration test: scan a known test directory, verify sizes
- [x] Benchmark: compare scan time vs `du -sh`, `dust`, `ncdu` on same directory

---

## Phase 2: GUI — Tree View + Treemap

- [x] Choose and set up GUI framework (egui/Slint/gtk4-rs)
- [x] Main window layout: tree view (left), treemap (bottom), toolbar (top)
- [x] Directory tree view widget with sortable columns
- [x] Squarified treemap rendering (custom widget)
- [x] Color mapping: file extension → color
- [x] Selection sync between tree and treemap
- [x] Breadcrumb navigation
- [x] Zoom in/out on treemap subtrees
- [ ] Open directory dialog
- [ ] Scan progress overlay
- [ ] Context menu (open folder, terminal, delete, properties)
- [ ] Keyboard navigation

---

## Phase 3: WizTree Feature Parity

- [ ] File Type View panel (table by extension: size, count, %)
- [ ] Top-1000 Largest Files view
- [ ] Top-1000 Largest Directories view
- [ ] Top-1000 Newest Files view
- [ ] Date range filter
- [ ] Search/filter bar (name, extension, size, date)
- [ ] CSV/JSON export
- [ ] Toolbar buttons for switching views

---

## Phase 4: io_uring + Fast Paths

- [ ] `io_uring` batched `statx()` backend
- [ ] `getdents64` via `io_uring` (kernel 6.x+)  
- [ ] Runtime kernel version detection
- [ ] Automatic backend selection (io_uring → sync fallback)
- [ ] XFS `bulkstat` ioctl integration
- [ ] btrfs `TREE_SEARCH` ioctl integration
- [ ] Benchmark suite across backends and filesystems

---

## Phase 5: Polish + QDirStat Parity

- [ ] Scan cache (save/load snapshots)
- [ ] Package manager integration (dpkg, rpm, pacman)
- [ ] Regex/glob exclude rules
- [ ] File age statistics histogram
- [ ] File size percentile statistics
- [ ] Unpackaged files view
- [ ] Cleanup actions
- [ ] Settings dialog
- [ ] Dark mode / themes
- [ ] i18n / localization
