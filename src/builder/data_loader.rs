//! Data loading abstractions for building ZDB files.
//!
//! This module provides the core data structures and traits for loading
//! dictionary entries from various sources during ZDB file construction.

use crate::Result;

/// Maximum length of a dictionary keyword in bytes.
pub const ZDB_MAX_KEYWORD_LENGTH: usize = 255;

/// Maximum length of a single dictionary entry (64MB).
pub const MAX_ENTRY_LEN: usize = 64 * 1024 * 1024;

/// Represents a single dictionary record during the build process.
///
/// This struct holds metadata about a dictionary entry, including its key,
/// content location, and size information. It's used as an intermediate
/// representation when converting from various source formats to ZDB.
#[derive(Debug, Clone, Default)]
pub struct ZdbRecord {
    /// The dictionary key (headword or search term)
    pub key: String,
    /// Offset of the content in the source data (calculated during build)
    pub content_offset_in_source: u64,
    /// Position in the source file where this entry was found
    pub position: u64,
    /// The content string, or filename for MDD archives
    pub content: String,
    /// Length of the content in bytes
    pub content_len: u64,
    /// Line number in the source file (for text-based sources)
    pub line_no: u64,
}

/// Common interface for loading dictionary entry data from various sources.
///
/// Implementations of this trait handle loading entry content from different
/// source formats (MDX files, text files, directories, etc.).
///
/// # Examples
///
/// ```no_run
/// use mdx::builder::data_loader::{DataLoader, ZdbRecord};
/// use mdx::Result;
///
/// struct MyDataLoader;
///
/// impl DataLoader for MyDataLoader {
///     fn load_data(&mut self, entry: &ZdbRecord) -> Result<Vec<u8>> {
///         // Load and return the entry content
///         Ok(entry.content.as_bytes().to_vec())
///     }
/// }
/// ```
pub trait DataLoader {
    /// Loads the content data for a given dictionary entry.
    ///
    /// # Arguments
    ///
    /// * `entry` - The record metadata describing which entry to load
    ///
    /// # Returns
    ///
    /// Returns the raw content bytes for the entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the data cannot be loaded from the source.
    fn load_data(&mut self, entry: &ZdbRecord) -> Result<Vec<u8>>;
}





