//! Error types and result type for the MDX crate.
//!
//! This module defines all error variants that can occur when working with MDX/MDD
//! dictionary files. It uses the `snafu` library for ergonomic error handling with
//! automatic backtrace capture.
//!
//! # Examples
//!
//! ```
//! use mdx::{Result, ZdbError};
//!
//! fn read_dictionary() -> Result<String> {
//!     // Return an error
//!     Err(ZdbError::invalid_parameter("Invalid dictionary path"))
//! }
//!
//! fn handle_error() {
//!     match read_dictionary() {
//!         Ok(data) => println!("Success: {}", data),
//!         Err(e) => eprintln!("Error: {}", e),
//!     }
//! }
//! ```
//!
//! # Error Variants
//!
//! - [`ZdbError::Io`]: I/O errors from file operations
//! - [`ZdbError::CrcMismatch`]: Data integrity check failures
//! - [`ZdbError::InvalidDataFormat`]: Malformed dictionary file data
//! - [`ZdbError::InvalidParameter`]: Invalid function parameters
//! - [`ZdbError::KeyNotFound`]: Dictionary key lookup failures
//! - [`ZdbError::CompressionError`]: Compression/decompression failures
//! - [`ZdbError::ParserError`]: XML/JSON parsing errors

use std::io;
use snafu::{Snafu, Backtrace};

// Re-export snafu for context providers
pub use snafu;

/// Main error type for the MDX crate.
///
/// All errors include automatic backtrace capture for debugging purposes.
/// Use the helper methods on `ZdbError` for convenient error construction.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ZdbError {
    /// I/O error occurred during file operations.
    #[snafu(display("IO error: {source}"))]
    Io {
        source: io::Error,
        backtrace: Backtrace,
    },
    
    /// CRC checksum validation failed, indicating data corruption.        
    #[snafu(display("CRC mismatch: expected {expected:#x}, got {got:#x}"))]
    CrcMismatch { 
        expected: u32, 
        got: u32,
        backtrace: Backtrace,
    },
    
    /// Error parsing XML, JSON, or other structured data formats.
	#[snafu(display("Parser error: {source}"))]
	ParserError {
		source: Box<dyn std::error::Error + Send + Sync + 'static>,
		backtrace: Backtrace,
	},
    
    /// Dictionary file data is malformed or doesn't match expected format.
    #[snafu(display("Invalid data format: {message}"))]
    InvalidDataFormat {
        message: String,
        backtrace: Backtrace,
    },

    /// Function was called with invalid parameters.
    #[snafu(display("Invalid parameter: {message}"))]
    InvalidParameter {
        message: String,
        backtrace: Backtrace,
    },

    /// ICU library error during collation or locale operations.
    #[snafu(display("Icu common error: {source}"))]
    IcuError {
        source: crate::utils::icu_wrapper::IcuError,
        backtrace: Backtrace,
    },

    /// Dictionary key was not found during lookup.
    #[snafu(display("Key not found: {key}"))]
    KeyNotFound {
        key: String,
        backtrace: Backtrace,
    },

    /// Dictionary profile ID was not found.
    #[snafu(display("Profile not found: {profile_id}"))]
    ProfileNotFound {
        profile_id: u32,
        backtrace: Backtrace,
    },

    /// Error during compression or decompression operations.
    #[snafu(display("Compression error: {message}"))]
    CompressionError {
        message: String,
        backtrace: Backtrace,
    },

    /// Operation was interrupted by user.
    #[snafu(display("User interrupted"))]
    UserInterrupted {
        backtrace: Backtrace,
    },

    /// General error that doesn't fit other categories.
    #[snafu(display("General error: {message}"))]
    GeneralError {
        message: String,
        backtrace: Backtrace,
    },
}

// For automatic conversions from standard error types
impl From<io::Error> for ZdbError {
    fn from(source: io::Error) -> Self {
        Self::Io { source, backtrace: Backtrace::capture() }
    }
}

impl From<serde_xml_rs::Error> for ZdbError {
    fn from(source: serde_xml_rs::Error) -> Self {
		Self::ParserError { source: Box::new(source), backtrace: Backtrace::capture() }
    }
}

impl From<quick_xml::Error> for ZdbError {
    fn from(source: quick_xml::Error) -> Self {
		Self::ParserError { source: Box::new(source), backtrace: Backtrace::capture() }
    }
}

impl From<std::string::FromUtf8Error> for ZdbError {
    fn from(source: std::string::FromUtf8Error) -> Self {
		Self::InvalidDataFormat { message: format!("Invalid UTF-8 (String): {}", source), backtrace: Backtrace::capture() }
    }
}

impl From<std::str::Utf8Error> for ZdbError {
    fn from(source: std::str::Utf8Error) -> Self {
		Self::InvalidDataFormat { message: format!("Invalid UTF-8 (&str): {}", source), backtrace: Backtrace::capture() }
    }
}

impl From<url::ParseError> for ZdbError {
    fn from(source: url::ParseError) -> Self {
        Self::ParserError { source: Box::new(source), backtrace: Backtrace::capture() }
    }
}

impl From<crate::utils::icu_wrapper::IcuError> for ZdbError {
    fn from(source: crate::utils::icu_wrapper::IcuError) -> Self {
        Self::IcuError { source, backtrace: Backtrace::capture() }
    }
}

impl From<std::num::ParseIntError> for ZdbError {
    fn from(source: std::num::ParseIntError) -> Self {
        Self::ParserError {
            source: Box::new(source),
            backtrace: Backtrace::capture(),
        }
    }
}

impl From<serde_json::Error> for ZdbError {
    fn from(source: serde_json::Error) -> Self {
        Self::ParserError {
            source: Box::new(source),
            backtrace: Backtrace::capture(),
        }
    }
}


/// Helper methods for creating errors without context providers.
impl ZdbError {
    /// Creates an `InvalidParameter` error with the given message.
    ///
    /// # Examples
    ///
    /// ```
    /// use mdx::ZdbError;
    ///
    /// let error = ZdbError::invalid_parameter("Path cannot be empty");
    /// ```
    pub fn invalid_parameter<S: Into<String>>(message: S) -> Self {
        Self::InvalidParameter {
            message: message.into(),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates an `InvalidDataFormat` error with the given message.
    pub fn invalid_data_format<S: Into<String>>(message: S) -> Self {
        Self::InvalidDataFormat {
            message: message.into(),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `KeyNotFound` error for the given key.
    pub fn key_not_found<S: Into<String>>(key: S) -> Self {
        Self::KeyNotFound {
            key: key.into(),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `ProfileNotFound` error for the given profile ID.
    pub fn profile_not_found(profile_id: u32) -> Self {
        Self::ProfileNotFound {
            profile_id,
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates an `InvalidParameter` error for an invalid path.
    pub fn invalid_path<S: Into<String>>(path: S) -> Self {
        Self::InvalidParameter {
            message: format!("Invalid path: {}", path.into()),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `CompressionError` with the given message.
    pub fn compression_error<S: Into<String>>(message: S) -> Self {
        Self::CompressionError {
            message: message.into(),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `CompressionError` for decompression failures.
    pub fn decompression_error<S: Into<String>>(message: S) -> Self {
        Self::CompressionError {
            message: message.into(),
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `UserInterrupted` error.
    pub fn user_interrupted() -> Self {
        Self::UserInterrupted {
            backtrace: Backtrace::capture(),
        }
    }

    /// Creates a `CrcMismatch` error with expected and actual CRC values.
    pub fn crc_mismatch(expected: u32, got: u32) -> Self {
        Self::CrcMismatch {
            expected,
            got,
            backtrace: Backtrace::capture(),
        }
    }

    /// Checks if this error is a `KeyNotFound` variant.
    pub fn is_key_not_found(&self) -> bool {
        if let ZdbError::KeyNotFound { .. } = self {
            return true;
        }
        false
    }

    /// Creates a `GeneralError` with the given message.
    pub fn general_error<S: Into<String>>(message: S) -> Self {
        Self::GeneralError {
            message: message.into(),
            backtrace: Backtrace::capture(),
        }
    }
}

/// A specialized `Result` type for MDX operations.
///
/// This is a convenience type alias that uses [`ZdbError`] as the error type.
pub type Result<T> = std::result::Result<T, ZdbError>;