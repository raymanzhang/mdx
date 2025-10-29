// Storage and core data structures for compressed/encrypted dictionary data
//
// This module provides structures and functions for managing storage blocks,
// content blocks, and core data structures used throughout the library.

pub mod meta_unit;
pub mod unit_base;
pub mod key_unit;
pub mod key_block;
pub mod key_block_index;
pub mod key_block_index_unit;
pub mod storage_block;
pub mod content_block;
pub mod content_block_index_unit;
pub mod content_unit;
pub mod zip_directory;
pub mod reader_helper;

pub use meta_unit::MetaUnit;
pub use unit_base::UnitType;
pub use key_block::{KeyIndex, KeyBlock, EntryNo};
pub use key_block_index::KeyBlockIndex;
pub use key_block_index_unit::KeyBlockIndexUnit;
pub use storage_block::StorageBlock;
pub use content_block::ContentBlock;
pub use content_block_index_unit::ContentBlockIndex;
pub use content_unit::ContentUnit;
pub use zip_directory::ZipDirectory;
pub use reader_helper::{UintReader};