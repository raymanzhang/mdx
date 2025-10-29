//! Progress reporting for long-running operations.
//!
//! This module provides a simple progress reporting mechanism for operations
//! like building dictionaries or indexing content. It allows callers to
//! receive periodic updates and optionally cancel operations.
//!
//! # Examples
//!
//! ```
//! use mdx::progress_report::{ProgressState, ProgressReportFn};
//!
//! fn my_reporter(state: &mut ProgressState) -> bool {
//!     println!("Progress: {}/{}", state.current, state.total);
//!     false // Return true to cancel the operation
//! }
//!
//! let mut progress = ProgressState::new("building", 100, 10, Some(my_reporter));
//! for i in 0..100 {
//!     if progress.report(i) {
//!         // Operation was cancelled
//!         break;
//!     }
//! }
//! ```

/// Function type for progress reporting callbacks.
///
/// The function receives a mutable reference to the progress state and
/// returns `true` to cancel the operation, or `false` to continue.
pub type ProgressReportFn= fn(&mut ProgressState) -> bool;

/// State information for progress reporting.
///
/// This struct tracks the progress of a long-running operation and
/// calls a reporter function at regular intervals.
pub struct ProgressState{
    /// Identifier for this progress state (e.g., "building", "indexing")
    pub state_id: String,
    /// Total number of items to process
    pub total: u64,
    /// Error message if an error occurred
    pub error_msg: String,
    /// Current item being processed
    pub current: u64,
    /// Last item at which progress was reported
    pub last: u64,
    /// Number of items between progress reports
    pub report_interval: u64,
    /// Optional reporter function to call
    pub reporter: Option<ProgressReportFn>,
}

impl ProgressState {
    /// Creates a new progress state.
    ///
    /// # Arguments
    ///
    /// * `state_id` - Identifier for this progress state
    /// * `total` - Total number of items to process
    /// * `report_interval_percent` - Percentage of items between reports (0-100)
    /// * `reporter` - Optional reporter function
    ///
    /// # Examples
    ///
    /// ```
    /// use mdx::progress_report::ProgressState;
    ///
    /// // Report every 10% of progress
    /// let progress = ProgressState::new("building", 1000, 10, None);
    /// ```
    pub fn new(state_id:&str, total: u64, report_interval_percent: u64, reporter: Option<ProgressReportFn>) -> Self {
        Self {
            state_id:state_id.to_string(),
            total,
            error_msg: String::new(),
            current:0,
            last: 0,
            report_interval: total*report_interval_percent/100,
            reporter,
        }
    }

    /// Reports progress for the current item.
    ///
    /// This method checks if enough items have been processed since the last
    /// report, and if so, calls the reporter function.
    ///
    /// # Arguments
    ///
    /// * `current` - The current item number being processed
    ///
    /// # Returns
    ///
    /// Returns `true` if the operation should be cancelled, `false` otherwise.
    pub fn report(&mut self, current: u64) -> bool {
        if self.reporter.is_none() {
            return false;
        }
        if (current-self.last) > self.report_interval || current == self.total-1 {
            self.current = current;
            let cancelled= (self.reporter.unwrap())(self); 
            self.last = current;
            return cancelled;
        }else{
            return false;
        }
    }
}
