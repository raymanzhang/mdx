//! MDD file URL utilities for handling dictionary resource paths.
//!
//! This module provides utility functions for working with file URLs used in
//! MDict dictionary files. It includes:
//! - URL path decoding and encoding
//! - File name and stem extraction
//! - Multi-part MDD file handling
//! - URL extension manipulation
//! - URL path joining
//!
//! # Examples
//!
//! ```no_run
//! use mdx::url_utils;
//! use url::Url;
//!
//! let url = Url::parse("file:///dict/Oxford%20English.mdx")?;
//! 
//! // Extract the file name
//! let file_name = url_utils::get_decoded_file_name(&url)?;
//! assert_eq!(file_name, "Oxford English.mdx");
//!
//! // Extract the file stem (without extension)
//! let file_stem = url_utils::get_decoded_file_stem(&url)?;
//! assert_eq!(file_stem, "Oxford English");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::path::{Path, PathBuf};

use percent_encoding::percent_decode_str;
use url::Url;

use crate::utils::io_utils::fix_windows_path;
use crate::{Result, ZdbError};


pub fn get_decoded_path(url: &Url) -> Result<PathBuf> {
    let path_str = get_decoded_path_str(url)?;
    Ok(PathBuf::from(path_str))
}

pub fn get_decoded_path_str(url: &Url) -> Result<String> {
    let path = fix_windows_path(url.path());
    let decoded_path = percent_decode_str(&path)
        .decode_utf8()?;
    Ok(decoded_path.to_string())
}


/// Extracts and decodes the file name from a URL
///
/// This function takes a URL, extracts the file name from its path, and decodes any
/// percent-encoded characters. Returns an error if the file name is missing or cannot
/// be decoded properly.
///
/// # Examples
///
/// ```
/// use url::Url;
/// use mdx::url_utils::get_decoded_file_name;
///
/// let url = Url::parse("file:///path/to/my%20file.txt").unwrap();
/// let file_name = get_decoded_file_name(&url);
/// assert_eq!(file_name.unwrap(), "my file.txt");
/// ```
///
/// # Errors
///
/// Returns `ZdbError::InvalidDataFormat` if the URL path does not contain a file name.
/// Returns `ZdbError::InvalidDataFormat` if the file name contains invalid characters.
/// Returns `ZdbError::InvalidDataFormat` if percent-decoding fails.
pub fn get_decoded_file_name(url: &Url) -> Result<String> {
    // Extract the path from the URL
    let path = url.path();

    // Get the file name from the path
    let file_name = Path::new(path)
        .file_name()
        .ok_or(ZdbError::invalid_data_format(format!(
            "Can't find file name in url: {}",
            url.to_string()
        )))?
        .to_str()
        .ok_or(ZdbError::invalid_data_format(format!(
            "Invalid unicode encoding in url: {}",
            url.to_string()
        )))?;

    // Decode percent-encoded characters
    Ok(percent_decode_str(file_name).decode_utf8()?.into_owned())
}

/// Extracts and decodes the file stem from a URL
///
/// This function takes a URL, extracts the file stem (file name without extension) from its path,
/// and decodes any percent-encoded characters. Returns an error if the file stem is missing or cannot
/// be decoded properly.
///
/// # Examples
///
/// ```
/// use url::Url;
/// use mdx::url_utils::get_decoded_file_stem;
///
/// let cases = [
///     ("file:///path/to/my%20file.txt", "my file"),
///     ("file:///file.tar.gz", "file.tar"),
/// ];
///
/// for (input, expected) in cases {
///     let url = Url::parse(input).unwrap();
///     let file_stem = get_decoded_file_stem(&url).unwrap();
///     assert_eq!(file_stem, expected);
/// }
/// ```
pub fn get_decoded_file_stem(url: &Url) -> Result<String> {
    let path = url.path();
    let file_stem = Path::new(path)
        .file_stem()
        .ok_or(ZdbError::invalid_data_format(format!(
            "Can't find file stem in url: {}",
            url
        )))?
        .to_str()
        .ok_or(ZdbError::invalid_data_format(format!(
            "Invalid unicode encoding in url: {}",
            url
        )))?;
    Ok(percent_decode_str(file_stem).decode_utf8()?.into_owned())
}

/// Joins two URLs by keeping the first URL's scheme, host, and port, but replacing its path
/// with the result of joining the first URL's path (without filename) with the second URL's path.
/// Query and fragment are taken from the second URL.
///
/// This function extracts the path from the second URL and uses `join_path` to combine it
/// with the first URL's directory path (filename removed).
///
/// # Examples
///
/// ```
/// use url::Url;
/// use mdx::url_utils::join_url_path;
///
/// let cases = [
///     (
///         "https://example.com/path/to/file.txt",
///         "https://other.com/new/path",
///         "https://example.com/path/to/new/path",
///     ),
///     (
///         "https://example.com/path/to/file.txt?old=1",
///         "https://other.com/new/path?key=value",
///         "https://example.com/path/to/new/path?key=value",
///     ),
///     (
///         "https://example.com/path/to/file.txt#old",
///         "https://other.com/new/path#section",
///         "https://example.com/path/to/new/path#section",
///     ),
///     (
///         "https://example.com/path/to/",
///         "https://other.com/new/path",
///         "https://example.com/path/to/new/path",
///     ),
///     (
///         "https://example.com/path/file_without_extension",
///         "https://other.com/new/path",
///         "https://example.com/path/new/path",
///     ),
///     (
///         "https://example.com/file.txt",
///         "https://other.com/new/path",
///         "https://example.com/new/path",
///     ),
/// ];
///
/// for (first, second, expected) in cases {
///     let first_url = Url::parse(first).unwrap();
///     let second_url = Url::parse(second).unwrap();
///     let result = join_url_path(&first_url, &second_url).unwrap();
///     assert_eq!(result.as_str(), expected);
/// }
/// ```
pub fn join_url_path(first_url: &Url, second_url: &Url) -> Result<Url> {
    // Clone the base URL
    let mut base = first_url.clone();

    // Get the path from the first URL
    let first_path = first_url.path();
    
    // Get the path from the second URL
    let second_path = second_url.path();
        
    // Use join_path to combine the first URL's path (without filename) with second URL's path
    let joined_path = join_path(first_path, second_path);

    // Set the new path and copy query/fragment from second URL
    base.set_path(&joined_path);
    base.set_query(second_url.query());
    base.set_fragment(second_url.fragment());

    Ok(base)
}

/// Joins two path strings by removing the filename from the first path and appending the second path.
///
/// This function takes a base path, removes the filename part (if any), and then joins it with
/// the second path. If the second path doesn't start with "/", it will be appended with a "/".
///
/// # Arguments
/// * `base_path` - The base path from which to remove the filename
/// * `second_path` - The path to append to the base path (without filename)
///
/// # Returns
/// A new string containing the joined path
///
/// # Examples
///
/// ```
/// use mdx::url_utils::join_path;
///
/// // Remove filename from first path and join with second path
/// assert_eq!(join_path("/path/to/file.txt", "new/path"), "/path/to/new/path");
/// assert_eq!(join_path("/path/to/file.txt", "/new/path"), "/path/to/new/path");
/// assert_eq!(join_path("/path/to/", "new/path"), "/path/to/new/path");
/// assert_eq!(join_path("/path/to/", "/new/path"), "/path/to/new/path");
/// assert_eq!(join_path("/", "new/path"), "/new/path");
/// assert_eq!(join_path("/", "/new/path"), "/new/path");
/// ```
pub fn join_path(base_path: &str, second_path: &str) -> String {
    // Remove filename from base_path
    let base_without_filename = if let Some(last_slash) = base_path.rfind('/') {
        if last_slash == 0 {
            "/"  // Keep root if it's just "/"
        } else {
            &base_path[..last_slash + 1]  // Include the trailing slash
        }
    } else {
        base_path  // No slash found, use as-is
    };
    
    // Ensure base path ends with "/" (except for root)
    let base_dir = if base_without_filename == "/" {
        "/".to_string()
    } else if base_without_filename.ends_with('/') {
        base_without_filename.to_string()
    } else {
        format!("{}/", base_without_filename)
    };
    
    // Join with second path
    if second_path.starts_with('/') {
        // Remove leading slash from second path and join
        let trimmed_second = second_path.trim_start_matches('/');
        if base_dir == "/" {
            format!("/{}", trimmed_second)
        } else {
            format!("{}{}", base_dir, trimmed_second)
        }
    } else {
        // Second path is relative, join directly
        if base_dir == "/" {
            format!("/{}", second_path)
        } else {
            format!("{}{}", base_dir, second_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_path() {
        // Test cases for join_path function
        let test_cases = [
            // (base_path, second_path, expected)
            // Remove filename from first path and join with second path
            ("/path/to/file.txt", "new/path", "/path/to/new/path"),
            ("/path/to/file.txt", "/new/path", "/path/to/new/path"),
            ("/path/to/", "new/path", "/path/to/new/path"),
            ("/path/to/", "/new/path", "/path/to/new/path"),
            ("/", "new/path", "/new/path"),
            ("/", "/new/path", "/new/path"),
            ("/base/file.txt", "subdir/file.txt", "/base/subdir/file.txt"),
            ("/base/file.txt", "/subdir/file.txt", "/base/subdir/file.txt"),
            ("/base/", "subdir/file.txt", "/base/subdir/file.txt"),
            ("/base/", "/subdir/file.txt", "/base/subdir/file.txt"),
            ("/file.txt", "new/path", "/new/path"),
            ("/file.txt", "/new/path", "/new/path"),
            ("file.txt", "new/path", "file.txt/new/path"),  // No leading slash case
        ];

        for (base, second, expected) in test_cases {
            let result = join_path(base, second);
            assert_eq!(result, expected, "join_path({:?}, {:?}) should be {:?}, got {:?}", 
                      base, second, expected, result);
        }
    }
}

/// Replaces the extension of the last path segment in a URL with a new extension.
///
/// This function takes a parsed `Url` and a new extension, replacing the existing
/// extension (if any) in the last path segment. If there is no path segment, the
/// new extension is appended to the path. The function preserves query parameters
/// and fragments.
///
/// # Arguments
/// * `url` - A reference to a parsed `Url` object.
/// * `new_ext` - The new extension to apply (with or without a leading dot).
///
/// # Returns
/// * `Ok(Url)` - The modified URL with the new extension.
/// * `Err(ZdbError)` - If the URL cannot be modified (e.g., invalid path segments).
///
/// # Examples
///
/// ```
/// use url::Url;
/// use mdx::url_utils::with_extension;
///
/// let cases = [
///     ("https://example.com/file.txt", "jpg", "https://example.com/file.jpg"),
///     ("https://example.com/file.txt?query=1", "jpg", "https://example.com/file.jpg?query=1"),
///     ("https://example.com/file", "jpg", "https://example.com/file.jpg"),
///     ("https://example.com/file.txt#fragment", "jpg", "https://example.com/file.jpg#fragment"),
///     ("https://example.com/file.txt?query=1#fragment", "jpg", "https://example.com/file.jpg?query=1#fragment"),
///     ("https://example.com/file.tar.gz", "bz2", "https://example.com/file.tar.bz2"),
///     ("https://example.com/file", ".jpg", "https://example.com/file.jpg"),
///     ("https://example.com/", "jpg", "https://example.com/.jpg"),
///     ("file://localhost/file.txt", "jpg", "file:///file.jpg"), 
///     //file://localhost/file.txt is a special case, it will be converted to file:///file.txt
/// ];
///
/// for (input, ext, expected) in cases {
///     let url = Url::parse(input).unwrap();
///     let result = with_extension(&url, ext).unwrap();
///     assert_eq!(result.as_str(), expected);
/// }
/// ```
pub fn with_extension(url: &Url, new_ext: &str) -> Result<Url> {
    // Clone URL for modification
    let mut parsed_url = url.clone();

    // Trim leading dot from new extension
    let new_ext = new_ext.trim_start_matches('.');

    // Get all segments as a vector from the original URL
    let segments: Vec<String> = url
        .path_segments()
        .map(|segments| segments.map(|s| s.to_string()).collect())
        .unwrap_or_default();

    if let Some(last) = segments.last() {
        // Replace extension in the last segment
        let new_last_segment = match last.rfind('.') {
            Some(dot_idx) => format!("{}.{}", &last[..dot_idx], new_ext),
            None => format!("{}.{}", last, new_ext),
        };

        // Get the original path to preserve any special prefixes (like //localhost)
        let original_path = url.path();
        let path_prefix = if let Some(idx) = original_path.rfind('/') {
            &original_path[..=idx]
        } else {
            "/"
        };

        // Construct the new path preserving the original prefix
        let new_path = format!("{}{}", path_prefix, new_last_segment);
        parsed_url.set_path(&new_path);
    } else {
        // If no path segments, append extension directly
        parsed_url.set_path(&format!("/.{}", new_ext));
    }

    Ok(parsed_url)
}

pub fn replace_url_path<P: AsRef<Path>>(url: &Url, path: P) -> Result<Url> {
    let path_str = path.as_ref().to_str().ok_or(ZdbError::invalid_data_format(format!("Invalid path: {}", path.as_ref().to_string_lossy())))?;
    let mut parsed_url = url.clone();
    parsed_url.set_path(path_str);
    Ok(parsed_url)
}