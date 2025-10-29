// Utility functions and helpers
//
// This module provides general-purpose utility functions including I/O operations,
// sorting, HTML rewriting, progress reporting, compression, and related helpers.

pub mod utils;
pub mod io_utils;
pub mod sort_key;
pub mod mdx_html_rewriter;
pub mod progress_report;
pub mod compression;
pub mod icu_wrapper;
pub mod url_utils;

pub use utils::{
    remove_xml_declaration,
    KeyComparable, RandomAccessable, sort_key_compare, locale_compare, 
    binary_search_first, key_compare, html_escape_mdx_text, extract_text_from_html,
    move_element
};
pub use io_utils::{read_exact_to_vec, scan_dir, windows_path_to_unix_path, fix_windows_path_buf};
pub use sort_key::get_sort_key;
pub use mdx_html_rewriter::MdxHtmlRewriter;
pub use progress_report::{ProgressState, ProgressReportFn};
pub use compression::{CompressionMethod, get_compressor};
pub use icu_wrapper::*;
pub use url_utils::*;
