//! # MDX - MDict Dictionary File Reader and Writer
//!
//! This crate provides a comprehensive library for reading and writing MDict dictionary files
//! (.mdx and .mdd formats), which are widely used for electronic dictionaries.
//!
//! ## Features
//!
//! - **Read MDX/MDD files**: Parse and extract dictionary entries and resources
//! - **Write ZDB files**: Create optimized dictionary databases with compression and encryption
//! - **Full-text search**: Build and query full-text search indexes using Tantivy
//! - **Multiple compression formats**: Support for LZ4, LZMA, Bzip2, LZO, and Zlib
//! - **Encryption support**: Handle encrypted dictionary files with various encryption methods
//! - **ICU collation**: Proper locale-aware sorting using ICU (optional feature)
//! - **HTML rewriting**: Process and rewrite HTML content with resource links
//!
//! ## Quick Start
//!
//! ### Reading an MDX Dictionary
//!
//! ```no_run
//! use mdx::readers::MdxReader;
//! use url::Url;
//!
//! # fn main() -> mdx::Result<()> {
//! // Open an MDX dictionary file
//! let url = Url::parse("file:///path/to/dictionary.mdx")?;
//! let mut reader = MdxReader::from_url(&url, "device_id")?;
//!
//! // Look up a word
//! let key_index = reader.lookup("hello")?;
//! let definition = reader.get_html(&key_index)?;
//! println!("Definition: {}", definition);
//! # Ok(())
//! # }
//! ```
//!
//! ### Building a ZDB Dictionary
//!
//! ```no_run
//! use mdx::builder::{ZDBBuilder, BuilderConfig, SourceType};
//! use std::path::PathBuf;
//!
//! # fn main() -> mdx::Result<()> {
//! let config = BuilderConfig::default();
//! let mut builder = ZDBBuilder::from_config(config);
//!
//! // Build from MDX source
//! builder.build(
//!     &PathBuf::from("source.mdx"),
//!     &PathBuf::from("output.zdb"),
//!     SourceType::Mdx,
//!     None,
//! )?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Feature Flags
//!
//! - `icu` (default): Use ICU4X for Unicode collation (pure Rust implementation)
//! - `rust-icu`: Use rust_icu for Unicode collation (requires system ICU library)
//!
//! ## Architecture
//!
//! The crate is organized into several key modules:
//!
//! - **Reader modules**: [`readers`] for reading MDX, MDD, and ZDB dictionary files
//! - **Builder modules**: [`builder`] for creating and converting dictionary files
//! - **Storage & core types**: [`storage`] for core data structures and storage management
//! - **Cryptography**: [`crypto`] for encryption operations
//! - **Utilities**: [`utils`] for helper functions and common operations
//!
//! ## Error Handling
//!
//! All fallible operations return a [`Result<T>`] type, where errors are represented by
//! [`ZdbError`]. The crate uses the `snafu` library for ergonomic error handling with
//! context and backtraces.
//!
//! ```
//! use mdx::{Result, ZdbError};
//!
//! fn example() -> Result<String> {
//!     // Operations that may fail return Result<T>
//!     Ok("success".to_string())
//! }
//! ```

pub mod builder;
pub mod crypto;
pub mod error;
pub mod readers;
pub mod storage;
pub mod utils;

// Re-export commonly used types for convenience
pub use readers::{MdxReader, MddReader, ZdbReader};
pub use storage::{MetaUnit, KeyIndex};

// Re-export error types for convenience
pub use error::{ZdbError, Result, snafu};

