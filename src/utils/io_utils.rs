//! I/O utility functions for file and URL operations.
//!
//! This module provides helper functions for:
//! - File URL handling and path conversion
//! - Cross-platform path normalization (Windows/Unix)
//! - Reading files and directories
//! - String and binary data loading
//! - ZDB version detection
//!
//! # Examples
//!
//! ```no_run
//! use mdx::io_utils::{bytes_from_file, string_from_file_url};
//! use url::Url;
//!
//! // Read bytes from a file
//! let data = bytes_from_file("dictionary.zdb").unwrap();
//!
//! // Read string from file URL
//! let url = Url::parse("file:///path/to/file.txt").unwrap();
//! let mut content = String::new();
//! string_from_file_url(&url, &mut content).unwrap();
//! ```

use std::collections::LinkedList;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use regex::Regex;
use url::Url;
use walkdir::WalkDir;

use crate::utils::url_utils;
use crate::{Result, ZdbError};

/// Fixes Windows file paths by removing the leading slash.
///
/// Under Windows, file URLs look like "file:///C:/Users/test/Desktop/test.txt",
/// so we need to remove the leading "/" to get a valid Windows path.
///
/// # Arguments
///
/// * `path` - The path string to fix
///
/// # Returns
///
/// Returns the fixed path string (unchanged on non-Windows platforms).
pub fn fix_windows_path(path: &str) -> String {
    #[cfg(target_os = "windows")]
    if path.len()>4 {
        let chars: Vec<char> = path.chars().take(4).collect();
        if chars[0]=='/' && chars[1].is_alphabetic() && chars[2]==':'{
            path = path.strip_prefix("/").unwrap();
        }
    }
    path.to_string()
}

/// Fixes Windows file paths in a PathBuf.
///
/// See [`fix_windows_path`] for details.
pub fn fix_windows_path_buf(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    let path = PathBuf::from(fix_windows_path(&path.to_string_lossy()));
    path
}

/// Converts Windows-style backslashes to Unix-style forward slashes.
pub fn windows_path_to_unix_path(path: &str) -> String {
    let mut result = String::new();
    for c in path.chars() {
        if c == '\\' {
            result.push('/');
        } else {
            result.push(c);
        }
    }
    result
}

/// Checks if a file URL points to an existing file.
pub fn file_url_exists(url: &Url) -> bool {
    url_utils::get_decoded_path(url).is_ok_and(
        |path| 
            Path::new(&path).exists()
    )
}

/// Opens a file URL and returns a buffered reader.
///
/// # Errors
///
/// Returns an error if the URL scheme is not "file" or the file cannot be opened.
pub fn open_file_url_as_reader(url: &Url) -> Result<BufReader<std::fs::File>> {
    if url.scheme() != "file" {
        return Err(ZdbError::invalid_data_format(format!(
            "Unsupported scheme: {}",
            url.scheme()
        )));
    }
    let path = fix_windows_path_buf(url_utils::get_decoded_path(url)?);
    let file = File::open(path)?;
    Ok(BufReader::new(file))
}

/// Reads all bytes from a file URL.
pub fn bytes_from_file_url(url: &Url) -> Result<Vec<u8>> {
    let mut reader = open_file_url_as_reader(url)?;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Reads a string from a file URL into the provided buffer.
pub fn string_from_file_url(url: &Url, str: &mut String) -> Result<()> {
    let mut reader = open_file_url_as_reader(url)?;
    reader.read_to_string(str)?;
    Ok(())
}

/// Reads all bytes from a file path.
pub fn bytes_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    Ok(buffer)
}

pub fn load_string_from_file_with_ext(base_url: &Url, ext: &str) -> Result<String> {
    let key_file_url = url_utils::with_extension(base_url, ext)?;
    if file_url_exists(&key_file_url) {
        let mut str_buf = String::new();
        string_from_file_url(&key_file_url, &mut str_buf)?;
        Ok(str_buf)
    } else {
        Ok(String::new())
    }
}

pub fn read_exact_to_vec<R: Read>(reader: &mut R, len:usize) -> crate::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(len as usize);
    unsafe {
        buf.set_len(buf.capacity()); // Set length without initializing memory
    }
    reader.read_exact(buf.as_mut_slice())?;
    Ok(buf)
} 

pub fn copy_optimized<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> Result<u64> {
    let mut total_bytes = 0;
    loop {
        let buf = reader.fill_buf()?; // Get reference to underlying buffer
        if buf.is_empty() {
            break; // End of reading
        }
        writer.write_all(buf)?; // Write buffer contents directly
        let len = buf.len() as u64;
        total_bytes += len;
        reader.consume(len as usize); // Mark buffer as consumed
    }
    writer.flush()?; // Ensure all data is written
    Ok(total_bytes)
}

/// Scan a directory for files matching the given regex pattern
/// 
/// # Arguments
/// * `target_dir` - The directory to scan
/// * `pattern` - A regex pattern to match file names against
/// * `recursive` - Whether to scan subdirectories recursively
/// * `files` - A list to store the matching file paths
/// 
/// # Returns
/// Returns `Ok(true)` on success, or an error if the scan fails
pub fn scan_dir<P: AsRef<Path>>(
    target_dir: P,
    pattern: &Regex,
    recursive: bool,
    files: &mut LinkedList<PathBuf>,
) -> Result<bool> {
    let walker = if recursive {
        WalkDir::new(&target_dir).follow_links(true).into_iter()
    } else {
        WalkDir::new(&target_dir).follow_links(true).max_depth(1).into_iter()
    };
    
    for entry in walker {
        let entry = entry.map_err(|e| ZdbError::invalid_data_format(format!("Walk directory error: {}", e)))?;
        
        // Skip directories, only process files (follow_links(true) will automatically handle symbolic links)
        if entry.file_type().is_file() {
            // In non-recursive mode, only process files at depth 1 (directly in the target directory)
            if !recursive && entry.depth() > 1 {
                continue;
            }
            
            let file_name = entry.file_name().to_str().ok_or_else(|| 
                ZdbError::invalid_data_format("Invalid file name encoding")
            )?;
            
            if pattern.is_match(file_name) {
                files.push_back(entry.path().to_path_buf());
            }
        }
    }
    Ok(true)
}