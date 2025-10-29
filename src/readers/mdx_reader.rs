//! MDX dictionary reader for high-level dictionary lookups.
//!
//! This module provides the main API for reading MDX (MDict) dictionary files.
//! It handles:
//! - Dictionary content lookup and retrieval
//! - Optional resource file (MDD) loading
//! - Full-text search support
//! - HTML link rewriting
//! - Compact stylesheet decompression
//!
//! # Examples
//!
//! ```no_run
//! use mdx::mdx_reader::MdxReader;
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
//! # Features
//!
//! - **Dictionary Lookup**: Fast key-based lookup with support for partial matches
//! - **Content Retrieval**: Get content in raw bytes, plain text, or HTML format
//! - **Full-Text Search**: Search through dictionary content if an index is available
//! - **Resource Loading**: Automatically load associated MDD files for images and media
//! - **HTML Rewriting**: Convert internal links to MDX protocol format

use std::collections::LinkedList;
use std::fs::File;
use std::io::BufReader;

use log::*;
use mime_guess::MimeGuess;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value;
use tantivy::{Index, TantivyDocument};
use url::Url;

use crate::utils::io_utils::{load_string_from_file_with_ext, open_file_url_as_reader};
use crate::storage::key_block::{EntryNo, KeyIndex};
use crate::utils::url_utils::{self, with_extension};
use super::mdd_reader::MddReader;
use crate::storage::meta_unit::ContentType;
use crate::utils::html_escape_mdx_text;
use super::zdb_reader::ZdbReader;
use crate::storage::zip_directory::ZipDirectory;
use crate::{Result, ZdbError};
const MDICT_INDEX_EXT: &str = "idx";
const MDICT_MDD_EXT: &str = "mdd";
const MDICT_KEY_EXT: &str = "key";

/// High-level MDX dictionary reader.
///
/// This struct provides the main interface for reading MDict (MDX) dictionary files.
/// It manages the content database, optional resource database, and full-text search index.
pub struct MdxReader {
    /// The main content database reader
    pub content_db: ZdbReader<BufReader<File>>,
    /// Optional associated resource (MDD) file reader
    pub data_db: Option<MddReader>,
    /// Optional full-text search index
    pub fts_index: Option<Index>,
    /// Name of the dictionary
    pub db_name: String,
    /// URL to the MDX file
    pub mdx_url: Url,
    /// Compact stylesheet for decompacting content
    compact_stylesheet: Vec<(String, String)>,
}

impl MdxReader {

    /// Opens an MDX dictionary file from a URL.
    ///
    /// This method attempts to load:
    /// 1. The main MDX content file
    /// 2. Associated MDD resource file (if exists)
    /// 3. Full-text search index (if exists)
    ///
    /// Missing optional resources do not cause failures; the reader will still function
    /// with those features unavailable.
    ///
    /// # Arguments
    ///
    /// * `mdx_url` - URL to the MDX file (typically `file:///path/to/file.mdx`)
    /// * `device_id` - Device identifier for license verification
    ///
    /// # Returns
    ///
    /// Returns an initialized MdxReader on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the main MDX file cannot be opened or parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mdx::mdx_reader::MdxReader;
    /// use url::Url;
    ///
    /// let url = Url::parse("file:///dict/Oxford.mdx")?;
    /// let reader = MdxReader::from_url(&url, "my_device")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_url(mdx_url: &Url, device_id: &str) -> Result<Self> {
        let mdx_url = mdx_url.clone();
        let reader = open_file_url_as_reader(&mdx_url)?;
        let license_data = load_string_from_file_with_ext(&mdx_url, MDICT_KEY_EXT)?;
        let content_db = ZdbReader::<BufReader<File>>::from_reader(reader, device_id, &license_data)?;
        
        // Try to initialize data_db, but allow it to fail
        let data_db = match MddReader::from_url(&with_extension(&mdx_url, MDICT_MDD_EXT)?, device_id) {
            Ok(db) => Some(db),
            Err(e) => {
                warn!("Failed to load MDD data database: {}. Data resources will not be available.", e);
                None
            }
        };
        
        let db_name= url_utils::get_decoded_file_stem(&mdx_url)?;
        let compact_stylesheet = Self::load_compact_stylesheet(&content_db.meta.db_info.style_sheet)?;
        
        // Try to initialize FTS index, but allow it to fail
        let fts_index = match Self::load_fts_index(&with_extension(&mdx_url, MDICT_INDEX_EXT)?) {
            Ok(index) => Some(index),
            Err(e) => {
                info!("Failed to load FTS index: {}. Full-text search will not be available.", e);
                None
            }
        };
        let mdx_reader = Self { content_db, data_db, fts_index, db_name, mdx_url, compact_stylesheet };
        Ok(mdx_reader)
    }


    /// Gets multiple key indexes starting from a specific entry number.
    ///
    /// # Arguments
    ///
    /// * `start_entry_no` - Starting entry number
    /// * `max_count` - Maximum number of entries to retrieve
    ///
    /// # Returns
    ///
    /// Returns a LinkedList of KeyIndex entries.
    pub fn get_indexes(&mut self, start_entry_no: EntryNo, max_count: u64) -> Result<LinkedList<KeyIndex>> {
        self.content_db.get_indexes(start_entry_no, max_count)
    }

    /// Gets a single key index by entry number.
    ///
    /// # Arguments
    ///
    /// * `entry_no` - Entry number to retrieve
    ///
    /// # Returns
    ///
    /// Returns the KeyIndex for the given entry number.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry number is invalid.
    pub fn get_index(&mut self, entry_no: EntryNo) -> Result<KeyIndex> {
        self.content_db.get_index(entry_no)
    }

    /// Gets raw (unprocessed) content bytes for a dictionary entry.
    ///
    /// # Arguments
    ///
    /// * `key_index` - The key index of the entry
    ///
    /// # Returns
    ///
    /// Returns the raw content bytes.
    pub fn get_raw(&mut self, key_index: &KeyIndex) -> Result<Vec<u8>> {
        self.content_db.get_data(key_index, false)
    } 

    /// Gets content as a string for a dictionary entry.
    ///
    /// This method can optionally decompact content that uses the compact stylesheet format.
    ///
    /// # Arguments
    ///
    /// * `key_index` - The key index of the entry
    /// * `decompact` - Whether to decompact the content using the stylesheet
    ///
    /// # Returns
    ///
    /// Returns the content as a UTF-8 string.
    pub fn get_string(&mut self, key_index: &KeyIndex, decompact: bool) -> Result<String> {
        if decompact && !self.compact_stylesheet.is_empty() {
			let compacted_content = self.content_db.get_string(key_index, true)?;
            Self::reformat(&compacted_content, &self.compact_stylesheet)
        } else {
            self.content_db.get_string(key_index, true)
        }
    }

    /// Gets content as HTML for a dictionary entry.
    ///
    /// This method automatically converts text content to HTML-escaped format
    /// and retrieves HTML content as-is.
    ///
    /// # Arguments
    ///
    /// * `key_index` - The key index of the entry
    ///
    /// # Returns
    ///
    /// Returns the content as HTML.
    ///
    /// # TODO
    ///
    /// Need to rebuild links in HTML to use mdx schema (mdx://)
    pub fn get_html(&mut self, key_index: &KeyIndex) -> Result<String> {
        //TODO Need to rebuild links in html to use mdx schema (mdx://)
        let content_type = self.content_db.meta.db_info.content_type.clone();
        match content_type {
            ContentType::Text => {
                let mut buffer = String::with_capacity(1024);
                html_escape_mdx_text(&self.get_string(key_index, true)?, &mut buffer);
                Ok(buffer)
            }
            ContentType::Html => {
                self.get_string(key_index, true)
            }
            _ => Err(ZdbError::invalid_data_format("Db content type is not supported")),
        }
    }

	/// Expand compacted content using stylesheet tokens surrounded by backticks.
	/// Tokens are specified as `number` where number is 0..255 and map to
	/// `compact_stylesheet[token] = (prefix, suffix)`.
    /// 
    pub fn reformat(compacted_source: &str, compact_style: &[(String, String)]) -> Result<String> {
        let mut chars = compacted_source.chars().peekable();
        let mut expanded_text = String::with_capacity(compacted_source.len()+1024);
        let mut processed_source= String::with_capacity(expanded_text.len()/2);
        while let Some(c) = chars.next() {
            if c == '`' {
                processed_source.push(c);
                let mut number = String::new();
                let mut has_number = false;
                while let Some(nc) = chars.next() {
                    processed_source.push(nc);
                    if nc.is_digit(10) {
                        number.push(nc);
                    } else if nc == '`' {
                        has_number = !number.is_empty();
                        break;
                    }else{
                        break;
                    }
                }
                if has_number {
                    //格式合法
                    let token = number.parse::<usize>().unwrap_or(256);
                    if token<256 {
                        processed_source.clear();
                        while let Some(nc) = chars.peek() {
                            if *nc != '`' {
                                processed_source.push(*nc);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        expanded_text.push_str(&compact_style[token].0);
                        expanded_text.push_str(&processed_source);
                        expanded_text.push_str(&compact_style[token].1);
                        processed_source.clear();
                    }
                }
                expanded_text.push_str(&processed_source);
                processed_source.clear();
            } else {
                expanded_text.push(c);
            }
        }
        expanded_text.push_str(&processed_source);
        Ok(expanded_text)
    }

    pub fn get_data(&mut self, file_path: &str) -> Result<Option<(Vec<u8>, String)>> {
        // Handle data database lookup
        if self.data_db.is_some() {
            let buffer = self.data_db.as_mut().unwrap().get_data_by_path(file_path, true)?;
            if let Some(buffer) = buffer {
                let mime_type = MimeGuess::from_path(file_path).first_or_octet_stream().to_string();
                    return Ok(Some((buffer, mime_type)));
            }
        }
        Ok(None)
    }

    pub fn get_entry_count(&self) -> u64 {
        self.content_db.get_entry_count()
    }

    pub fn find_index(&mut self, key: &str, prefix_match: bool, partial_match: bool, best_match: bool) -> Result<Option<KeyIndex>> {
        self.content_db.find_first_match(key, prefix_match, partial_match, best_match)
    }

    pub fn get_similar_indexes(&mut self, key_index: &KeyIndex, start_with: bool, max_count: u64) -> Result<LinkedList<KeyIndex>> {
        self.content_db.get_similar_indexes(key_index, start_with, max_count)
    }

    // Load compact stylesheet triples: token, prefix, suffix (newline-separated)
    pub fn load_compact_stylesheet(style_sheet: &str) -> Result<Vec<(String, String)>> {
        let mut compact_stylesheet = vec![(String::new(), String::new()); 256];
        let mut lines = style_sheet.split('\n').peekable();
        let mut has_stylesheet = false;
        loop {
            let token_line = match lines.next() {
                None => break,
                Some(line) => {
                    if line.is_empty() && lines.peek().is_none() {
                        break;
                    }
                    line
                }
            };

            let token: u32 = token_line
                .trim()
                .parse()
                .map_err(|_| ZdbError::invalid_data_format("Invalid token in compact stylesheet"))?;
            if token > 255 {
                return Err(ZdbError::invalid_data_format("Token out of range (0..255) in compact stylesheet"));
            }

            let prefix = lines
                .next()
                .ok_or(ZdbError::invalid_data_format("Unexpected end of compact stylesheet (missing prefix)"))?
                .to_string();
            let suffix = lines
                .next()
                .ok_or(ZdbError::invalid_data_format("Unexpected end of compact stylesheet (missing suffix)"))?
                .to_string();
            let prefix =htmlescape::decode_html(&prefix).map_err(|e| ZdbError::invalid_data_format(format!("Invalid HTML entity in compact stylesheet: {:?}", e)))?;
            let suffix =htmlescape::decode_html(&suffix).map_err(|e| ZdbError::invalid_data_format(format!("Invalid HTML entity in compact stylesheet: {:?}", e)))?;
            compact_stylesheet[token as usize] = ( prefix, suffix);
            has_stylesheet = true;
        }
        if !has_stylesheet {
            compact_stylesheet.clear();
        }
        Ok(compact_stylesheet)
    }

    /// Load FTS index from .idx file or directory
    fn load_fts_index(idx_url: &Url) -> Result<Index> {
        let idx_path = idx_url.to_file_path().map_err(|_| ZdbError::invalid_path("Invalid FTS index URL path".to_string()))?;
        
        // Check if .idx file exists first, then fall back to directory
        if idx_path.exists() {
            if idx_path.is_file() {
                let zip_directory = ZipDirectory::open(idx_path);
                Index::open(Box::new(zip_directory) as Box<dyn tantivy::directory::Directory>)
                    .map_err(|e| ZdbError::general_error(format!("Failed to open packed FTS index: {}", e)))
            } else {
                Index::open_in_dir(&idx_path)
                    .map_err(|e| ZdbError::general_error(format!("Failed to open FTS index directory: {}", e)))
            }
        } else {
            Err(ZdbError::general_error(format!("FTS index not found. Checked for: {}", idx_path.display())))
        }
    }

    
    /// Perform full-text search on the database content
    /// Returns a vector of (score, entry_no, key) tuples for matching entries
    pub fn fts_search(&self, query_str: &str, max_results: usize) -> Result<Vec<(f32, EntryNo, String)>> {
        if let Some(ref fts_index) = self.fts_index {
            // Create a searcher for searching
            let reader = fts_index.reader()
                .map_err(|e| ZdbError::general_error(format!("Failed to create FTS index reader: {}", e)))?;
            let searcher = reader.searcher();
            
            // Get schema fields
            let schema = fts_index.schema();
            let key_field = schema.get_field("key")
                .map_err(|_| ZdbError::general_error("Field 'key' not found in FTS schema".to_string()))?;
            let content_field = schema.get_field("content")
                .map_err(|_| ZdbError::general_error("Field 'content' not found in FTS schema".to_string()))?;
            let entry_no_field = schema.get_field("entry_no")
                .map_err(|_| ZdbError::general_error("Field 'entry_no' not found in FTS schema".to_string()))?;
            
            // Create query parser for the searchable fields  
            let query_parser = QueryParser::for_index(fts_index, vec![key_field, content_field]);
            
            // Parse the search query
            let query = query_parser.parse_query(query_str)
                .map_err(|e| ZdbError::general_error(format!("Failed to parse query '{}': {}", query_str, e)))?;
            
            // Perform the search
            let top_docs = searcher.search(&query, &TopDocs::with_limit(max_results))
                .map_err(|e| ZdbError::general_error(format!("FTS search failed: {}", e)))?;
            
            // Extract results
            let mut results = Vec::new();
            for (score, doc_address) in top_docs {
                // Retrieve the document from the index
                let retrieved_doc = searcher.doc::<TantivyDocument>(doc_address)
                    .map_err(|e| ZdbError::general_error(format!("Failed to retrieve document: {}", e)))?;
                
                // Extract fields from the document
                let entry_no: EntryNo = retrieved_doc
                    .get_first(entry_no_field)
                    .and_then(|v| v.as_u64())
                    .map(|n| n as EntryNo)
                    .ok_or(ZdbError::general_error("Entry number not found in FTS index".to_string()))?;
                let key = retrieved_doc.get_first(key_field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                    
                results.push((score, entry_no, key));
            }
            
            Ok(results)
        } else {
            Err(ZdbError::general_error("Full-text search index is not available".to_string()))
        }
    }
    
    /// Check if full-text search is available (index is loaded and not empty)
    pub fn is_fts_available(&self) -> bool {
        if let Some(ref fts_index) = self.fts_index {
            if let Ok(reader) = fts_index.reader() {
                reader.searcher().num_docs() > 0
            } else {
                false
            }
        } else {
            false
        }
    }
    
    /// Check if data database is available (for resources like CSS, images, etc.)
    pub fn is_data_db_available(&self) -> bool {
        self.data_db.is_some()
    }

}