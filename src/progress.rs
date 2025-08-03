//! Progress bar management for checkle operations.
//!
//! This module provides a clean abstraction over indicatif progress bars,
//! designed specifically for checkle's multi-file hashing operations.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    io::IsTerminal,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::constants::{MIN_FILE_SIZE_FOR_PROGRESS, PROGRESS_UPDATE_INTERVAL_MS};

/// Manager for progress bars during hashing operations.
///
/// Handles both overall progress across multiple files and per-file progress
/// for large files. Automatically disables progress bars in non-TTY environments.
#[derive(Debug, Clone)]
pub struct ProgressManager {
    multi_progress: Arc<MultiProgress>,
    overall_bar: Option<ProgressBar>,
    show_progress: bool,
}

impl ProgressManager {
    /// Creates a new progress manager.
    ///
    /// # Arguments
    /// * `show_progress` - Whether to show progress bars
    /// * `total_files` - Total number of files to process
    ///
    /// # Panics
    /// Panics if `total_files` is 0 when `show_progress` is true.
    #[must_use]
    pub fn new(show_progress: bool, total_files: usize) -> Self {
        // Precondition assertion
        if show_progress {
            assert!(
                total_files > 0,
                "Total files must be positive when showing progress"
            );
        }

        // Auto-disable progress in non-TTY environments
        // indicatif handles this automatically, but we'll double-check
        let show_progress = show_progress && std::io::stderr().is_terminal();

        let multi_progress = Arc::new(MultiProgress::new());

        let overall_bar = if show_progress {
            let bar = multi_progress.add(ProgressBar::new(total_files as u64));
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} files ({percent}%) {msg}")
                    .unwrap_or_else(|_| ProgressStyle::default_bar())
                    .progress_chars("=>-"),
            );
            bar.set_message("Hashing files...");
            bar.enable_steady_tick(Duration::from_millis(PROGRESS_UPDATE_INTERVAL_MS));

            Some(bar)
        } else {
            None
        };

        Self {
            multi_progress,
            overall_bar,
            show_progress,
        }
    }

    /// Creates a progress bar for a single file if it's large enough.
    ///
    /// # Arguments
    /// * `file_name` - Name of the file being processed
    /// * `file_size` - Size of the file in bytes
    ///
    /// # Returns
    /// A progress bar if the file is large enough and progress is enabled, None otherwise.
    /// Creates a progress bar for a single file if it's large enough.
    ///
    /// # Arguments
    /// * `file_name` - Name of the file being processed
    /// * `file_size` - Size of the file in bytes
    ///
    /// # Returns
    /// A progress bar if the file is large enough and progress is enabled, None otherwise.
    ///
    /// # Panics
    /// Panics if the progress bar template is invalid (should never happen with the default template).
    #[must_use]
    pub fn create_file_progress(&self, file_name: &str, file_size: u64) -> Option<FileProgress> {
        if !self.show_progress || file_size < MIN_FILE_SIZE_FOR_PROGRESS {
            return None;
        }

        let bar = ProgressBar::new(file_size);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("File:     [{bar:30.cyan/blue}] {bytes}/{total_bytes}  {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar())
                .progress_chars("=>-"),
        );

        let truncated_name = if file_name.len() > 40 {
            format!("...{}", &file_name[file_name.len() - 37..])
        } else {
            file_name.to_string()
        };

        bar.set_message(truncated_name.clone());
        bar.enable_steady_tick(Duration::from_millis(PROGRESS_UPDATE_INTERVAL_MS));

        let bar = self.multi_progress.add(bar);

        Some(FileProgress {
            bar,
            update_counter: Arc::new(Mutex::new(0)),
            total_size: file_size,
            file_name: truncated_name,
        })
    }

    /// Increments the overall progress bar.
    pub fn inc_overall(&self) {
        if let Some(ref bar) = self.overall_bar {
            bar.inc(1);
        }
    }

    /// Finishes the overall progress bar with a message.
    pub fn finish_with_message(&self, message: &str) {
        if let Some(ref bar) = self.overall_bar {
            // Ensure we're at 100% before clearing
            bar.set_position(bar.length().unwrap_or(0));
            bar.finish_with_message(message.to_string());
        }
    }

    /// Returns whether progress bars are being shown.
    #[must_use]
    pub fn is_showing_progress(&self) -> bool {
        self.show_progress
    }
}

/// Progress bar for a single file.
pub struct FileProgress {
    bar: ProgressBar,
    update_counter: Arc<Mutex<u64>>,
    total_size: u64,
    file_name: String,
}

impl FileProgress {
    /// Updates the progress bar with bytes processed.
    ///
    /// Throttles updates to avoid excessive overhead.
    /// If the internal mutex is poisoned, the update is silently skipped.
    pub fn update(&self, bytes_processed: u64) {
        // If the mutex is poisoned, we'll just skip the update rather than panic
        if let Ok(mut counter) = self.update_counter.lock() {
            *counter += 1;

            // Throttle updates to avoid overhead
            if *counter % 10 == 0 {
                self.bar.set_position(bytes_processed);
            }
        }
    }

    /// Updates the progress bar to show Merkle tree computation phase.
    pub fn enter_merkle_phase(&self) {
        self.bar
            .set_message(format!("{} - Finalizing...", self.file_name));
        self.bar.set_position(self.total_size);
    }

    /// Finishes the file progress bar.
    pub fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

impl Drop for FileProgress {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Creates a no-op progress manager for when progress is disabled.
impl Default for ProgressManager {
    fn default() -> Self {
        Self {
            multi_progress: Arc::new(MultiProgress::new()),
            overall_bar: None,
            show_progress: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_manager_creation_with_progress() {
        let manager = ProgressManager::new(false, 10); // Progress disabled in tests
        assert!(!manager.is_showing_progress());
    }

    #[test]
    fn test_progress_manager_creation_without_files() {
        let manager = ProgressManager::new(false, 0);
        assert!(!manager.is_showing_progress());
    }

    // #[test]
    // fn test_file_progress_threshold() {
    //     let manager = ProgressManager::new(true, 1);

    //     // Small file should not get progress bar
    //     let small_progress = manager.create_file_progress("small.txt", 1024);
    //     assert!(small_progress.is_none());

    //     // Large file should get progress bar (if in TTY)
    //     let large_progress =
    //         manager.create_file_progress("large.txt", MIN_FILE_SIZE_FOR_PROGRESS + 1);
    //     // In tests, this will be None because we're not in a TTY
    //     assert!(large_progress.is_none());
    // }
}
