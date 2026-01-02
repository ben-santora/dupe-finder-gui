mod scanner;

use eframe::egui;
use scanner::{
    scan_directory, FileInfo, ScanProgress, ScanPhase, ScanConfig, ScanError,
    SelectionStrategy, KeepNewestStrategy, KeepOldestStrategy
};
use std::fs;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use serde::{Deserialize, Serialize};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    
    eframe::run_native(
        "DupeFinder",
        options,
        Box::new(|_cc| Ok(Box::new(DupeFinderApp::default()))),
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub files: Vec<FileInfo>,
    pub selected: Vec<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    pub selected_dir: String,
    pub scanning: bool,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub total_size_savings: u64,
    pub status_message: String,
    pub config: ScanConfig,
    pub preview_mode: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_dir: String::new(),
            scanning: false,
            duplicate_groups: Vec::new(),
            total_size_savings: 0,
            status_message: String::new(),
            config: ScanConfig::default(),
            preview_mode: false,
        }
    }
}

struct DupeFinderApp {
    state: AppState,
    scan_progress: Arc<Mutex<Option<ScanProgress>>>,
    result_receiver: Option<Receiver<Result<Vec<Vec<FileInfo>>, ScanError>>>,
}

impl Default for DupeFinderApp {
    fn default() -> Self {
        Self {
            state: AppState::default(),
            scan_progress: Arc::new(Mutex::new(None)),
            result_receiver: None,
        }
    }
}

impl DupeFinderApp {
    fn start_scan(&mut self, ctx: &egui::Context) {
        if self.state.selected_dir.is_empty() || self.state.scanning {
            return;
        }
        
        self.state.scanning = true;
        self.state.duplicate_groups.clear();
        self.state.total_size_savings = 0;
        self.state.status_message.clear();
        
        let dir = self.state.selected_dir.clone();
        let progress = self.scan_progress.clone();
        let ctx_clone = ctx.clone();
        let config = self.state.config.clone();
        
        let (tx, rx) = channel();
        self.result_receiver = Some(rx);
        
        thread::spawn(move || {
            let progress_clone = progress.clone();
            let ctx_clone_2 = ctx_clone.clone();
            let result = scan_directory(&dir, move |p| {
                *progress_clone.lock().unwrap() = Some(p);
                ctx_clone_2.request_repaint();
            }, config);
            
            *progress.lock().unwrap() = None;
            let _ = tx.send(result);
            ctx_clone.request_repaint();
        });
    }
    
    fn calculate_savings(&mut self) {
        self.state.total_size_savings = 0;
        for group in &self.state.duplicate_groups {
            let files_to_delete: Vec<_> = group.files.iter()
                .zip(&group.selected)
                .filter(|(_, &selected)| !selected)
                .collect();
            
            for (file, _) in files_to_delete {
                self.state.total_size_savings += file.size;
            }
        }
    }
    
    fn delete_unchecked(&mut self, group_idx: usize) {
        if group_idx >= self.state.duplicate_groups.len() {
            return;
        }
        
        let group = &self.state.duplicate_groups[group_idx];
        let mut deleted_count = 0;
        let mut errors = Vec::new();
        let mut critical_files_found = Vec::new();
        
        if !self.state.preview_mode {
            for (_idx, (file, &keep)) in group.files.iter().zip(&group.selected).enumerate() {
                if !keep {
                    if file.is_critical {
                        critical_files_found.push(file.path.display().to_string());
                    }
                    match fs::remove_file(&file.path) {
                        Ok(_) => deleted_count += 1,
                        Err(e) => errors.push(format!("Failed to delete {}: {}", file.path.display(), e)),
                    }
                }
            }
        } else {
            // In preview mode, just count what would be deleted
            for (_idx, (file, &keep)) in group.files.iter().zip(&group.selected).enumerate() {
                if !keep {
                    if file.is_critical {
                        critical_files_found.push(file.path.display().to_string());
                    }
                    deleted_count += 1;
                }
            }
        }
        
        if errors.is_empty() {
            let action = if self.state.preview_mode { "Would delete" } else { "Deleted" };
            let mut message = format!("✓ {} {} file(s) from group {}", action, deleted_count, group_idx + 1);
            
            if !critical_files_found.is_empty() {
                message.push_str(&format!(" ⚠️ {} CRITICAL file(s) detected!", critical_files_found.len()));
                if self.state.preview_mode {
                    message.push_str(&format!(" Files: {}", critical_files_found.join(", ")));
                }
            }
            
            self.state.status_message = message;
            if !self.state.preview_mode {
                self.state.duplicate_groups.remove(group_idx);
                self.calculate_savings();
            }
        } else {
            self.state.status_message = format!("⚠ Errors: {}", errors.join("; "));
        }
    }
    
    fn apply_selection_strategy(&mut self, strategy: &dyn SelectionStrategy, group_idx: usize) {
        if let Some(group) = self.state.duplicate_groups.get_mut(group_idx) {
            group.selected = strategy.select(&group.files);
        }
        self.calculate_savings();
    }
    
    fn select_newest(&mut self, group_idx: usize) {
        self.apply_selection_strategy(&KeepNewestStrategy, group_idx);
    }
    
    fn select_oldest(&mut self, group_idx: usize) {
        self.apply_selection_strategy(&KeepOldestStrategy, group_idx);
    }
    
    fn bulk_apply_selection_strategy(&mut self, strategy: &dyn SelectionStrategy) {
        for group in &mut self.state.duplicate_groups {
            group.selected = strategy.select(&group.files);
        }
        self.calculate_savings();
    }
    
    fn bulk_select_newest(&mut self) {
        self.bulk_apply_selection_strategy(&KeepNewestStrategy);
    }
    
    fn bulk_select_oldest(&mut self) {
        self.bulk_apply_selection_strategy(&KeepOldestStrategy);
    }

    fn bulk_delete_unchecked(&mut self) {
        let mut deleted_count = 0;
        let mut errors = Vec::new();
        let mut groups_to_remove = Vec::new();
        let mut critical_files_found = Vec::new();

        for (group_idx, group) in self.state.duplicate_groups.iter().enumerate() {
            let mut group_deleted_count = 0;
            
            if !self.state.preview_mode {
                for (file, &keep) in group.files.iter().zip(&group.selected) {
                    if !keep {
                        if file.is_critical {
                            critical_files_found.push(file.path.display().to_string());
                        }
                        match fs::remove_file(&file.path) {
                            Ok(_) => {
                                deleted_count += 1;
                                group_deleted_count += 1;
                            },
                            Err(e) => errors.push(format!("Failed to delete {}: {}", file.path.display(), e)),
                        }
                    }
                }
            } else {
                // In preview mode, just count what would be deleted
                for (file, &keep) in group.files.iter().zip(&group.selected) {
                    if !keep {
                        if file.is_critical {
                            critical_files_found.push(file.path.display().to_string());
                        }
                        deleted_count += 1;
                        group_deleted_count += 1;
                    }
                }
            }
            
            // Only mark group for removal if files were actually deleted (or would be deleted in preview)
            if group_deleted_count > 0 {
                groups_to_remove.push(group_idx);
            }
        }

        if errors.is_empty() {
            let action = if self.state.preview_mode { "Would bulk delete" } else { "Bulk deleted" };
            let mut message = format!("✓ {} {} file(s) across {} group(s).", action, deleted_count, groups_to_remove.len());
            
            if !critical_files_found.is_empty() {
                message.push_str(&format!(" ⚠️ {} CRITICAL file(s) detected!", critical_files_found.len()));
                if self.state.preview_mode && critical_files_found.len() <= 5 {
                    message.push_str(&format!(" Files: {}", critical_files_found.join(", ")));
                } else if self.state.preview_mode {
                    message.push_str(&format!(" First 5: {}", critical_files_found.iter().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
                }
            }
            
            self.state.status_message = message;
            
            if !self.state.preview_mode {
                // Remove groups in reverse order to maintain indices
                for &group_idx in groups_to_remove.iter().rev() {
                    self.state.duplicate_groups.remove(group_idx);
                }
                self.calculate_savings();
            }
        } else {
            self.state.status_message = format!("⚠ Bulk delete finished with {} errors: {}", errors.len(), errors.iter().take(3).cloned().collect::<Vec<_>>().join("; "));
            if !self.state.preview_mode {
                // Still remove groups that were successfully processed
                for &group_idx in groups_to_remove.iter().rev() {
                    self.state.duplicate_groups.remove(group_idx);
                }
                self.calculate_savings();
            }
        }
    }
    
    fn export_results(&self) -> Result<String, String> {
        match serde_json::to_string_pretty(&self.state.duplicate_groups) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Failed to serialize results: {}", e)),
        }
    }
    
    fn import_results(&mut self, json: &str) -> Result<(), String> {
        match serde_json::from_str::<Vec<DuplicateGroup>>(json) {
            Ok(groups) => {
                self.state.duplicate_groups = groups;
                self.calculate_savings();
                self.state.status_message = format!("Imported {} duplicate group(s)", self.state.duplicate_groups.len());
                Ok(())
            },
            Err(e) => Err(format!("Failed to import results: {}", e)),
        }
    }
}

impl eframe::App for DupeFinderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for scan results
        if let Some(rx) = &self.result_receiver {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(groups) => {
                        self.state.duplicate_groups = groups.into_iter()
                            .map(|files| {
                                let selected = vec![true; files.len()];
                                DuplicateGroup { files, selected }
                            })
                            .collect();
                        self.state.scanning = false;
                        self.result_receiver = None;
                        self.calculate_savings();
                        
                        if self.state.duplicate_groups.is_empty() {
                            self.state.status_message = "No duplicates found.".to_string();
                        } else {
                            self.state.status_message = format!("Found {} duplicate group(s)!", self.state.duplicate_groups.len());
                        }
                    }
                    Err(e) => {
                        self.state.scanning = false;
                        self.result_receiver = None;
                        self.state.status_message = format!("Scan error: {:?}", e);
                    }
                }
            }
        }
        
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🔍 DupeFinder - Rust Duplicate File Finder");
            ui.add_space(10.0);
            
            // Directory selection
            ui.horizontal(|ui| {
                ui.label("Directory:");
                ui.add(egui::TextEdit::singleline(&mut self.state.selected_dir).desired_width(500.0));
                
                if ui.button("📁 Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.state.selected_dir = path.display().to_string();
                    }
                }
            });
            
            ui.add_space(10.0);
            
            // Configuration and controls
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.state.config.include_hidden, "Include hidden files");
                ui.checkbox(&mut self.state.preview_mode, "Preview mode (no actual deletion)");
                
                ui.add(egui::Slider::new(&mut self.state.config.buffer_size, 1024..=1048576)
                    .text("Buffer size"));
            });
            
            ui.add_space(10.0);
            
            // Scan button
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.state.scanning, egui::Button::new("🔍 Scan Directory")).clicked() {
                    self.start_scan(ctx);
                }
                
                if self.state.scanning {
                    ui.spinner();
                    ui.label("Scanning...");
                }
            });
            
            ui.add_space(10.0);
            
            // Progress bar
            if let Some(progress) = self.scan_progress.lock().unwrap().as_ref() {
                let fraction = progress.current as f32 / progress.total.max(1) as f32;
                let phase_text = match progress.phase {
                    ScanPhase::Discovery => "Discovering files",
                    ScanPhase::Hashing => "Hashing files",
                };
                ui.add(egui::ProgressBar::new(fraction)
                    .text(format!("{}: {} / {} files", phase_text, progress.current, progress.total)));
                
                let current_file = &progress.current_file;
                let display_path = if current_file.len() > 80 {
                    format!("...{}", &current_file[current_file.len()-77..])
                } else {
                    current_file.clone()
                };
                ui.label(format!("📄 {}", display_path));
            }
            
            // Status message
            if !self.state.status_message.is_empty() {
                ui.add_space(5.0);
                let color = if self.state.preview_mode {
                    egui::Color32::from_rgb(100, 150, 200) // Blue for preview mode
                } else {
                    egui::Color32::from_rgb(100, 200, 100) // Green for normal mode
                };
                ui.colored_label(color, &self.state.status_message);
            }
            
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            
            // Results
            if !self.state.duplicate_groups.is_empty() {
                // Check for critical files and show warning
                let critical_files_count: usize = self.state.duplicate_groups
                    .iter()
                    .map(|group| group.files.iter().filter(|f| f.is_critical).count())
                    .sum();
                
                if critical_files_count > 0 {
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), 
                            format!("⚠️ WARNING: {} critical system/user configuration files detected!", critical_files_count));
                        ui.colored_label(egui::Color32::from_rgb(200, 200, 100), 
                            "These files may be important for your system or applications.");
                    });
                    ui.add_space(5.0);
                }
                
                ui.horizontal(|ui| {
                    ui.heading(format!("📊 Found {} duplicate group(s)", self.state.duplicate_groups.len()));
                    ui.label("|");
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 100),
                        format!("💾 Potential savings: {:.2} MB", self.state.total_size_savings as f64 / 1_048_576.0)
                    );
                    if self.state.preview_mode {
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 150, 200),
                            "🔍 PREVIEW MODE"
                        );
                    }
                });
                
                ui.add_space(5.0);
                
                // Export/Import and Bulk actions
                ui.horizontal(|ui| {
                    ui.label("File Actions:");
                    if ui.button("📤 Export Results").clicked() {
                        match self.export_results() {
                            Ok(json) => {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("JSON", &["json"])
                                    .set_file_name("duplicate_results.json")
                                    .save_file() {
                                    if let Err(e) = std::fs::write(&path, json) {
                                        self.state.status_message = format!("Failed to save file: {}", e);
                                    } else {
                                        self.state.status_message = format!("Results exported to {}", path.display());
                                    }
                                }
                            }
                            Err(e) => {
                                self.state.status_message = e;
                            }
                        }
                    }
                    
                    if ui.button("📥 Import Results").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file() {
                            match std::fs::read_to_string(&path) {
                                Ok(json) => {
                                    match self.import_results(&json) {
                                        Ok(_) => {}, // Message set in import_results
                                        Err(e) => {
                                            self.state.status_message = e;
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.state.status_message = format!("Failed to read file: {}", e);
                                }
                            }
                        }
                    }
                });
                
                ui.add_space(5.0);
                
                // Bulk actions
                ui.horizontal(|ui| {
                    ui.label("Bulk Actions:");
                    if ui.button("📅 Keep Newest in All Groups").clicked() {
                        self.bulk_select_newest();
                    }
                    if ui.button("🕰 Keep Oldest in All Groups").clicked() {
                        self.bulk_select_oldest();
                    }
                    let delete_text = if self.state.preview_mode { "🔍 Preview Delete" } else { "🗑 Delete Unchecked" };
                    if ui.button(delete_text).clicked() {
                        self.bulk_delete_unchecked();
                    }
                });
                
                ui.add_space(10.0);
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut group_to_delete = None;
                    let mut recalculate = false;
                    let mut select_newest_for = None;
                    let mut select_oldest_for = None;
                    
                    for (group_idx, group) in self.state.duplicate_groups.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.strong(format!("Group {} ", group_idx + 1));
                                ui.label(format!("({} files, {:.2} MB each)", 
                                    group.files.len(),
                                    group.files[0].size as f64 / 1_048_576.0
                                ));
                            });
                            
                            ui.add_space(5.0);
                            
                            for (idx, file) in group.files.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let checkbox_response = ui.checkbox(&mut group.selected[idx], "Keep");
                                    if checkbox_response.changed() {
                                        recalculate = true;
                                    }
                                    
                                    // Show warning for critical files
                                    if file.is_critical {
                                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "⚠️ ");
                                    }
                                    
                                    ui.label(file.path.display().to_string());
                                    if let Some(modified) = file.modified_time {
                                        if let Ok(datetime) = modified.elapsed() {
                                            ui.label(format!("({} days ago)", datetime.as_secs() / 86400));
                                        }
                                    }
                                    
                                    if file.is_critical {
                                        ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "[CRITICAL]");
                                    }
                                });
                            }
                            
                            ui.add_space(5.0);
                            
                            ui.horizontal(|ui| {
                                if ui.button("📅 Keep Newest").clicked() {
                                    select_newest_for = Some(group_idx);
                                }
                                if ui.button("🕰 Keep Oldest").clicked() {
                                    select_oldest_for = Some(group_idx);
                                }
                                let delete_text = if self.state.preview_mode { "🔍 Preview Delete" } else { "🗑 Delete Unchecked" };
                                if ui.button(delete_text).clicked() {
                                    group_to_delete = Some(group_idx);
                                }
                            });
                        });
                        
                        ui.add_space(10.0);
                    }
                    
                    if recalculate {
                        self.calculate_savings();
                    }
                    
                    if let Some(idx) = select_newest_for {
                        self.select_newest(idx);
                    }
                    
                    if let Some(idx) = select_oldest_for {
                        self.select_oldest(idx);
                    }
                    
                    if let Some(idx) = group_to_delete {
                        self.delete_unchecked(idx);
                    }
                });
            } else if !self.state.scanning {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.label("Select a directory and click 'Scan Directory' to find duplicate files.");
                    ui.add_space(10.0);
                    ui.label("✓ Uses SHA-256 hashing for accurate detection");
                    ui.label("✓ Fast parallel processing with Rayon");
                    ui.label("✓ Configurable buffer size and hidden file handling");
                    ui.label("✓ Preview mode for safe testing");
                    ui.label("✓ Export/import scan results");
                    ui.label("✓ Cached file metadata for better performance");
                });
            }
        });
    }
}
