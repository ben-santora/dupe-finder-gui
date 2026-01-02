use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::{DirEntry, WalkDir};
use sha2::{Sha256, Digest};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
    pub modified_time: Option<SystemTime>,
    pub is_critical: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: String,
    pub phase: ScanPhase,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ScanPhase {
    Discovery,
    Hashing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanConfig {
    pub buffer_size: usize,
    pub include_hidden: bool,
    pub min_file_size: u64,
    pub max_threads: Option<usize>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            buffer_size: 65536, // 64KB buffer for better performance
            include_hidden: false,
            min_file_size: 1,
            max_threads: None,
        }
    }
}

#[derive(Debug)]
pub enum ScanError {
    IoError(io::Error),
    WalkdirError(walkdir::Error),
    HashError(String),
}

impl From<io::Error> for ScanError {
    fn from(err: io::Error) -> Self {
        ScanError::IoError(err)
    }
}

impl From<walkdir::Error> for ScanError {
    fn from(err: walkdir::Error) -> Self {
        ScanError::WalkdirError(err)
    }
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry.file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

fn is_critical_file(path: &Path) -> bool {
    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
        // List of critical system/user configuration files
        let critical_files = [
            ".bashrc", ".bash_profile", ".bash_logout", ".profile", ".zshrc", ".zprofile",
            ".vimrc", ".gvimrc", ".emacs", ".emacs.d", ".config", ".local", ".cache",
            ".ssh", ".gnupg", ".aws", ".docker", ".kube", ".npm", ".pip", ".conda",
            ".env", ".gitconfig", ".hgrc", ".subversion", ".tmux.conf", ".screenrc",
            ".Xauthority", ".xinitrc", ".xsession", ".xprofile", ".xrc", ".Xresources",
            ".gtkrc", ".xmodmap", ".inputrc", ".netrc", ".lesshst", ".python_history",
            ".mysql_history", ".psql_history", ".sqlite_history", ".rvm", ".rbenv",
            ".cargo", ".rustup", ".gradle", ".m2", ".ivy2", ".sbt", ".coursier",
            ".lein", ".boot", ".clojure", ".cider", ".nrepl-history", ".calibredb",
            ".thunderbird", ".mozilla", ".chromium", ".google-chrome", ".opera",
            ".vlc", ".audacity-data", ".gimp", ".inkscape", ".blender", ".kde",
            ".gnome", ".cinnamon", ".mate", ".xfce4", ".lxde", ".fluxbox",
            ".i3", ".sway", ".bspwm", ".dwm", ".xmonad", ".herbstluftwm",
            ".config/nvim", ".config/vim", ".config/emacs", ".config/fish",
            ".config/zsh", ".config/bash", ".config/git", ".config/ssh",
            ".config/gtk-3.0", ".config/gtk-4.0", ".config/kdeglobals",
            ".config/plasma", ".config/xfce4", ".config/i3", ".config/sway",
        ];
        
        // Check if the file name or any parent directory is critical
        if critical_files.contains(&filename) {
            return true;
        }
        
        // Check if any parent directory is critical
        for ancestor in path.ancestors() {
            if let Some(ancestor_name) = ancestor.file_name().and_then(|n| n.to_str()) {
                if critical_files.contains(&ancestor_name) {
                    return true;
                }
            }
        }
    }
    false
}

fn get_file_metadata(path: &Path) -> io::Result<(u64, Option<SystemTime>)> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();
    let modified = metadata.modified().ok();
    Ok((size, modified))
}

pub fn scan_directory<F>(dir: &str, progress_callback: F, config: ScanConfig) -> Result<Vec<Vec<FileInfo>>, ScanError>
where
    F: Fn(ScanProgress) + Send + Sync + 'static,
{
    let mut files_by_size: HashMap<u64, Vec<(PathBuf, Option<SystemTime>, bool)>> = HashMap::new();
    let mut total_files = 0;

    // Phase 1: Discovery
    let walker = WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| config.include_hidden || !is_hidden(e));

    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            match get_file_metadata(entry.path()) {
                Ok((size, modified)) => {
                    if size >= config.min_file_size {
                        let path = entry.path().to_path_buf();
                        let is_critical = is_critical_file(&path);
                        files_by_size.entry(size).or_default().push((path, modified, is_critical));
                        total_files += 1;
                    }
                }
                Err(_) => {
                    // Skip files we can't read, but continue scanning
                    continue;
                }
            }
        }
    }

    progress_callback(ScanProgress {
        current: total_files,
        total: total_files,
        current_file: "Discovery complete".to_string(),
        phase: ScanPhase::Hashing,
    });

    // Filter to only files with potential duplicates
    let potential_duplicates: Vec<_> = files_by_size
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();

    let mut duplicates: Vec<Vec<FileInfo>> = Vec::new();
    let mut processed_count = 0;

    for (size, paths_with_time) in potential_duplicates {
        let paths: Vec<PathBuf> = paths_with_time.iter().map(|(p, _, _)| p.clone()).collect();
        
        // Parallel hashing using rayon
        let hash_results: Vec<(PathBuf, Result<String, ScanError>)> = paths
            .par_iter()
            .map(|path| {
                let path_clone = path.clone();
                let config_clone = config.clone();
                let _local_processed = 0;
                
                let hash_result = move || {
                    hash_file(&path_clone, &config_clone)
                        .map_err(|e| ScanError::HashError(format!("Failed to hash {}: {}", path_clone.display(), e)))
                };

                let result = hash_result();
                (path.clone(), result)
            })
            .collect();

        let mut files_by_hash: HashMap<String, Vec<(PathBuf, Option<SystemTime>, bool)>> = HashMap::new();

        for ((path, hash_result), (_, time, is_critical)) in hash_results.into_iter().zip(paths_with_time) {
            processed_count += 1;
            progress_callback(ScanProgress {
                current: processed_count,
                total: total_files,
                current_file: path.display().to_string(),
                phase: ScanPhase::Hashing,
            });

            if let Ok(hash) = hash_result {
                files_by_hash.entry(hash).or_default().push((path, time, is_critical));
            }
        }

        for (_, paths_with_time) in files_by_hash {
            if paths_with_time.len() > 1 {
                let group: Vec<FileInfo> = paths_with_time
                    .into_iter()
                    .map(|(path, modified, is_critical)| FileInfo { path, size, modified_time: modified, is_critical })
                    .collect();
                duplicates.push(group);
            }
        }
    }

    Ok(duplicates)
}

fn hash_file(path: &Path, config: &ScanConfig) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; config.buffer_size];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hex::encode(hasher.finalize()))
}

// Selection strategies
pub trait SelectionStrategy {
    fn select(&self, files: &[FileInfo]) -> Vec<bool>;
}

pub struct KeepNewestStrategy;
pub struct KeepOldestStrategy;
pub struct KeepAllStrategy;
pub struct KeepNoneStrategy;

impl SelectionStrategy for KeepNewestStrategy {
    fn select(&self, files: &[FileInfo]) -> Vec<bool> {
        let mut selected = vec![false; files.len()];
        if let Some((newest_idx, _)) = files.iter()
            .enumerate()
            .max_by_key(|(_, f)| f.modified_time) {
            selected[newest_idx] = true;
        }
        selected
    }
}

impl SelectionStrategy for KeepOldestStrategy {
    fn select(&self, files: &[FileInfo]) -> Vec<bool> {
        let mut selected = vec![false; files.len()];
        if let Some((oldest_idx, _)) = files.iter()
            .enumerate()
            .min_by_key(|(_, f)| f.modified_time) {
            selected[oldest_idx] = true;
        }
        selected
    }
}

impl SelectionStrategy for KeepAllStrategy {
    fn select(&self, files: &[FileInfo]) -> Vec<bool> {
        vec![true; files.len()]
    }
}

impl SelectionStrategy for KeepNoneStrategy {
    fn select(&self, files: &[FileInfo]) -> Vec<bool> {
        vec![false; files.len()]
    }
}
