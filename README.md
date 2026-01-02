# dupe-finder-gui - Duplicate File Finder

A fast and efficient GUI application for finding and removing duplicate files

---

## 🚀 Features

### Performance & Architecture
- **Parallel Processing**: Uses Rayon for concurrent file hashing for faster scans
- **Cached Metadata**: Stores file modification times during scanning to avoid repeated filesystem calls
- **Optimized Memory Usage**: Better memory management with configurable buffer sizes
- **Comprehensive Error Handling**: Proper error propagation with user-friendly messages

### Functionality
- **Preview Mode**: Test deletion operations without actually deleting files
- **Export/Import Results**: Save scan results to JSON and reload them later
- **Critical File Protection**: Automatic detection and warning for important system/user configuration files
- **Configuration Options**: 
  - Adjustable buffer size (1KB - 1MB) for optimal performance
  - Toggle hidden file inclusion
  - Configurable minimum file size
- **Progress Tracking**: Shows discovery vs hashing phases
- **File Age Display**: Shows how old each file is in days

### User Interface
- **State Management**: Clean UI with proper state separation
- **Visual Indicators**: Preview mode highlighting and clear status messages
- **Bulk Operations**: Only removes groups where files were actually deleted
- **Selection Strategies**: Extensible selection strategies for file keeping
- **Critical File Warnings**: Red highlighting and warnings for important system files

---

## ✨ Core Features

* GUI built with eframe / egui for a native desktop experience
* Traverses directories recursively (walkdir)
* Groups files by size to avoid unnecessary hashing
* Uses SHA-256 to detect identical file contents
* Real-time scan progress with phase indicators
* Per-group and bulk actions:
  * Keep newest / oldest in a group
  * Toggle individual files as "Keep"
  * Delete unchecked files for a group or all groups
* Shows estimated potential disk space savings
* Export/import scan results for later analysis

---

## 🔧 Requirements

* Rust toolchain (1.70+ recommended)
* Platform-specific GUI dependencies (handled by the used crates)
* Crates used (Cargo.toml includes):
  * eframe / egui - GUI framework
  * walkdir - Directory traversal
  * sha2 - SHA-256 hashing
  * hex - Hex encoding
  * rfd - Native file dialogs
  * rayon - Parallel processing
  * serde / serde_json - Serialization
  * tokio - Async runtime

---

## 🛠️ Build & Run

### From Source
Clone the repository:
    git clone https://github.com/your-username/dupe-finder-gui.git  
    cd dupe-finder-gui

Run in debug mode:
    cargo run

Build optimized release binary:
    cargo build --release

Run the release binary:
    ./target/release/dupe-finder-gui


**Notes:**
* For best performance, use `--release` build
* On macOS, additional build tools may be required
* The Rust toolchain and system C linker are typically sufficient

---

## 📖 Usage

*** ⚠️ Always use Preview Mode first to review what will be deleted! ***

1. **Start the application**
2. **Configure options** (optional):
   - Enable/disable hidden file scanning
   - Adjust buffer size for performance
   - Enable Preview Mode for safe testing
3. **Select directory** using Browse button or enter path
4. **Click "Scan Directory"** to find duplicates
5. **Review results**:
   - Groups show identical files with age information
   - Potential disk space savings are calculated
6. **Make selections**:
   - Use checkboxes to keep specific files
   - Use "Keep Newest/Oldest" for automatic selection
   - Use bulk actions for all groups
7. **Preview or Delete**:
   - In Preview Mode: See what would be deleted
   - Normal Mode: Actually delete unchecked files
8. **Export results** (optional): Save scan results to JSON for later

### Configuration Options
- **Buffer Size**: 1KB - 1MB (default 64KB) - Larger buffers = faster but more memory
- **Include Hidden Files**: Scan hidden files and directories
- **Preview Mode**: Show what would be deleted without actual deletion
- **Export/Import**: Save and reload scan results

---

## 🔄 Performance

### Technical Details
- Parallel file hashing using Rayon thread pool
- Cached file metadata eliminates redundant filesystem calls
- Strategy pattern for extensible selection algorithms
- Proper error propagation and recovery
- Memory-efficient streaming for large directories

---

## 🛡️ Safety Notes

- **Preview Mode**: Always test in Preview Mode before actual deletion
- **Critical File Protection**: Automatically detects and warns about important system files:
  - Shell configurations (.bashrc, .zshrc, .profile, etc.)
  - Application settings (.config, .local, .cache, etc.)
  - Development environments (.cargo, .rustup, .npm, .pip, etc.)
  - System configurations (.ssh, .gnupg, .aws, .docker, etc.)
  - Desktop environments (.gnome, .kde, .xfce4, etc.)
  - Browser profiles (.mozilla, .chromium, .google-chrome, etc.)
- **Visual Warnings**: Critical files are highlighted with red ⚠️ indicators and [CRITICAL] labels
- **Deletion Alerts**: Shows count and names of critical files that would be deleted
- **Backups**: Ensure important data is backed up before bulk operations
- **Permissions**: Some files may require elevated permissions to delete
- **System Files**: Be careful when scanning system directories

The application attempts file deletions using standard filesystem APIs. Permission errors or locked files will be reported with detailed error messages.

---

## 📊 Algorithm Details

1. **Discovery Phase**: Recursively walk directory, group files by size
2. **Hashing Phase**: Parallel SHA-256 hashing of same-sized files
3. **Grouping**: Files with identical hashes are grouped as duplicates
4. **Selection**: User chooses which files to keep in each group
5. **Deletion**: Unchecked files are removed (with preview option)

The algorithm groups by size first and then uses SHA-256 to eliminate false positives and avoid unnecessary hashing operations.

---

## 📄 License

GPLv3

---

## 🙏 Acknowledgements

- eframe / egui — immediate-mode GUI in Rust
- walkdir — recursive directory traversal
- rayon — data parallelism library
- sha2 — SHA-256 hashing
- serde — serialization framework
- rfd — native file dialogs

---

## 👤 Creator

Ben Santora
