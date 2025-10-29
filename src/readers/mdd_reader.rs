//! MDD (resource) file reader for accessing dictionary resources.
//!
//! This module provides reader support for MDD files, which store images, audio,
//! and other resources referenced by MDX dictionary files. It handles:
//! - Single and multi-part MDD files
//! - Resource lookup by file path or key
//! - Content override from the filesystem
//!
//! # Examples
//!
//! ```no_run
//! use mdx::mdd_reader::MddReader;
//! use url::Url;
//!
//! # fn main() -> mdx::Result<()> {
//! let mdd_url = Url::parse("file:///dict/resources.mdd")?;
//! let mut reader = MddReader::from_url(&mdd_url, "device_id")?;
//!
//! // Get resource by key
//! if let Some(data) = reader.get_data_by_key("img/picture.png")? {
//!     println!("Found image: {} bytes", data.len());
//! }
//! # Ok(())
//! # }
//! ```

use std::cell::RefCell;
use std::collections::LinkedList;
use std::fs::File;
use std::io::BufReader;

use url::Url;

use crate::utils::io_utils::{bytes_from_file_url, file_url_exists, load_string_from_file_with_ext, open_file_url_as_reader};
use crate::utils::url_utils;
use super::zdb_reader::ZdbReader;
use crate::Result;

/// Reader for MDD (resource) files.
///
/// This struct provides access to resources stored in MDD files, including images,
/// audio files, and other binary data referenced by MDX dictionary files.
/// It supports both single MDD files and multi-part MDD files (e.g., `.mdd`, `.1.mdd`, `.2.mdd`).
pub struct MddReader {
    /// Base URL for the MDD files
    mdd_base_url: Url,
    /// Database name
    _db_name: String,
    /// List of ZDB readers for multi-part MDD files
    zdb_readers: RefCell<LinkedList<ZdbReader<BufReader<std::fs::File>>>>,
}

impl Default for MddReader {
    fn default() -> Self {
        Self {mdd_base_url: Url::parse("file:///").unwrap(), _db_name: String::new(), zdb_readers: RefCell::new(LinkedList::new())}
    }
}

impl MddReader {
    /// Reads a file from the same location as the MDD file.
    ///
    /// This method attempts to load a file from the same directory as the MDD file.
    /// Returns an empty vector if the file does not exist.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Name of the file to read
    ///
    /// # Returns
    ///
    /// Returns the file contents as bytes.
    pub fn read_file_from_same_location(&self, file_name: &str) -> Result<Vec<u8>> {
        let file_url = self.mdd_base_url.clone().join(file_name)?;
        if file_url_exists(&file_url) {
            Ok(bytes_from_file_url(&file_url)?)
        }else{
            Ok(Vec::new())
        }
    }

    /// Opens an MDD resource file from a URL.
    ///
    /// This method loads the main MDD file and any multi-part files (e.g., `.1.mdd`, `.2.mdd`).
    ///
    /// # Arguments
    ///
    /// * `mdd_url` - URL to the MDD file
    /// * `device_id` - Device identifier for license verification
    ///
    /// # Returns
    ///
    /// Returns an initialized MddReader on success.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mdx::mdd_reader::MddReader;
    /// use url::Url;
    ///
    /// let url = Url::parse("file:///dict/Oxford_English.mdd")?;
    /// let reader = MddReader::from_url(&url, "device_id")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_url(mdd_url: &Url, device_id: &str) -> Result<Self> {
        let mut zdb_readers = LinkedList::new();
        let license_data = load_string_from_file_with_ext(mdd_url, "key")?;
        if file_url_exists(&mdd_url) {
            let reader = open_file_url_as_reader(mdd_url)?;
            let zdb_reader = ZdbReader::<BufReader<File>>::from_reader(reader, device_id, &license_data)?;
            zdb_readers.push_back(zdb_reader);
        }
        let db_name= url_utils::get_decoded_file_stem(&mdd_url)?;

        let mdd_base_url= mdd_url.clone();
        for i in 1..100{
            let mdd_url = url_utils::with_extension(&mdd_base_url, &format!("{}.mdd", i))?; // File names are base.mdd, base.1.mdd, base.2.mdd, ...
            if file_url_exists(&mdd_url) {
                let reader = open_file_url_as_reader(&mdd_url)?;
                let zdb_reader = ZdbReader::<BufReader<File>>::from_reader(reader, device_id, &license_data)?;
                zdb_readers.push_back(zdb_reader);
            }
        }
        Ok(Self {mdd_base_url, _db_name: db_name, zdb_readers: RefCell::new(zdb_readers)})
    }
    
    /// Gets resource data by file path, with optional override capability.
    ///
    /// This method first checks for overrides in the local filesystem, then searches
    /// the MDD file(s) by key.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the resource file
    /// * `allow_override` - If true, check for local filesystem overrides first
    ///
    /// # Returns
    ///
    /// Returns `Some(data)` if found, `None` if not found.
    pub fn get_data_by_path(&mut self, file_path: &str, allow_override: bool) -> Result<Option<Vec<u8>>> {
        if allow_override {
            let file_url = Url::parse(&format!("file://{}", file_path))?;
            let override_url = url_utils::join_url_path(&self.mdd_base_url, &file_url)?;
            if file_url_exists(&override_url) {
                return Ok(Some(bytes_from_file_url(&override_url)?));
            }
        }
        self.get_data_by_key(file_path)    
    }

    /// Gets resource data by key from the MDD file(s).
    ///
    /// This method searches through all loaded MDD file(s) for the given key.
    /// For v2 files, Windows paths are automatically converted to Unix paths.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Key path for the resource
    ///
    /// # Returns
    ///
    /// Returns `Some(data)` if found, `None` if not found.
    pub fn get_data_by_key(&mut self, file_path: &str) -> Result<Option<Vec<u8>>> {
        if self.zdb_readers.borrow().front().is_none() {
            return Ok(None);
        }
        let actual_file_path = if !self.zdb_readers.borrow().front().unwrap().meta.is_v3() {
            // Convert unix path to windows path
            file_path.replace("/", "\\")
        } else {
            file_path.to_string()
        };

        for zdb_reader in self.zdb_readers.borrow_mut().iter_mut() {
            let result = zdb_reader.get_data_by_key(&actual_file_path);
            match result {
                Ok(data) => {
                    if data.is_some(){
                        return Ok(data);
                    }else {
                        // Key not found
                        continue;
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        return Ok(None);
    }
}