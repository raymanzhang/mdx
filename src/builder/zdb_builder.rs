//! ZDB dictionary builder for creating and converting dictionary files.
//!
//! This module provides the core functionality for building ZDB (MDict/MDD) files
//! from various source formats. It handles:
//!
//! - Configuration management for build parameters
//! - Dictionary entry collection and organization
//! - Key block and content block index generation
//! - Entry writing with compression and encryption
//! - Support for multiple source formats (MDX, MDD, text files, directories)
//!
//! # Overview
//!
//! The ZDB builder process involves:
//! 1. Creating a configuration with build parameters
//! 2. Loading entries from the source format
//! 3. Organizing entries into blocks with indexes
//! 4. Writing blocks with optional compression and encryption
//! 5. Creating metadata and finalizing the file
//!
//! # Source Types
//!
//! - `MdictHtml`: MDX format with HTML content
//! - `MdictCompact`: Compact MDX format
//! - `Directory`: Build from directory structure
//! - `Zdb`: Convert from existing ZDB format
//! - And others (StarDict, Kdic, SGD, etc.)
//!
//! # Examples
//!
//! ## Basic Dictionary Building
//!
//! ```no_run
//! use mdx::builder::{ZDBBuilder, BuilderConfig, SourceType};
//! use std::path::PathBuf;
//!
//! # fn main() -> mdx::Result<()> {
//! // Create configuration
//! let mut config = BuilderConfig::default();
//! config.input_path = "/path/to/source.mdx".to_string();
//! config.output_file = "/path/to/output.zdb".to_string();
//! config.data_source_format = SourceType::MdictHtml;
//! config.content_type = "Html".to_string();
//! config.default_sorting_locale = "en_US".to_string();
//!
//! // Build the dictionary
//! ZDBBuilder::build_with_config(&config, None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Building with Progress Reporting
//!
//! ```no_run
//! use mdx::builder::BuilderConfig;
//! use mdx::builder::ZDBBuilder;
//! use mdx::progress_report::ProgressState;
//!
//! # fn main() -> mdx::Result<()> {
//! fn report_progress(state: &mut ProgressState) -> bool {
//!     println!("Progress: {}/{}", state.current, state.total);
//!     false // Return true to cancel
//! }
//!
//! let config = BuilderConfig::default();
//! ZDBBuilder::build_with_config(&config, Some(report_progress))?;
//! # Ok(())
//! # }
//! ```
//!
//! ## JSON Configuration
//!
//! ```no_run
//! use mdx::builder::{BuilderConfig, ZDBBuilder};
//! use std::fs;
//!
//! # fn main() -> mdx::Result<()> {
//! // Load configuration from JSON
//! let json_content = fs::read_to_string("config.json")?;
//! let config: BuilderConfig = serde_json::from_str(&json_content)
//!     .map_err(|e| mdx::ZdbError::general_error(e.to_string()))?;
//!
//! // Build with configuration
//! ZDBBuilder::build_with_config(&config, None)?;
//! # Ok(())
//! # }
//! ```

use std::io::{Seek, Write};

use byteorder::{BigEndian, LittleEndian, WriteBytesExt};
use log::*;
use serde::{Deserialize, Serialize};

use crate::builder::data_loader::ZdbRecord;
use crate::builder::zdb_unit_builder::ZdbUnitBuilder;
use crate::utils::compression::CompressionMethod;
use crate::storage::content_block_index_unit::ContentBlockIndex;
use crate::crypto::digest::fast_hash_digest;
use crate::crypto::encryption::EncryptionMethod;
use crate::utils::icu_wrapper::UCollator;
use crate::storage::key_block::EntryNo;
use crate::storage::key_block_index::KeyBlockIndex;
use crate::utils::progress_report::{ProgressReportFn, ProgressState};
use crate::storage::unit_base::UnitType;
use crate::utils::remove_xml_declaration;
use crate::{Result, ZdbError};

/// Source dictionary format type.
///
/// Specifies the format of the input source when building a ZDB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    /// SGD format (105)
    Sgd = 105,
    /// Compact MDX format (106)
    MdictCompact = 106, 
    /// MDX HTML format (107)
    MdictHtml = 107,
    /// SugarDict with phonetic (110)
    SugarDictWithPhonetic = 110,
    /// StarDict format (111)
    StarDict = 111,
    /// Kdic format (112)
    Kdic = 112,
    /// Existing ZDB format (113)
    Zdb = 113,
    /// Directory structure (114)
    Directory = 114,
}

/// Configuration for building ZDB dictionaries.
///
/// Contains all parameters needed to build a dictionary file,
/// including input/output paths, compression settings, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderConfig{
    /// Path to the input source file or directory
    pub input_path: String,
    /// Path to the output ZDB file
    pub output_file: String,
    /// Whether registration is by email
    pub register_by_email: bool,
    /// Password for the dictionary (if applicable)
    pub password: String,
    /// Format of the source data
    pub data_source_format: SourceType,
    /// Type of content (Html, Text, or Binary)
    pub content_type: String,
    /// Default locale for sorting (e.g., "en_US", "zh_CN")
    pub default_sorting_locale: String,
    /// Preferred size for content blocks (default: 64KB)
    pub preferred_content_block_size: u32,
    /// Preferred size for key blocks (default: 16KB)
    pub preferred_key_block_size: u32,

    /// Device ID for encryption (not serialized)
    #[serde(skip)]
    pub device_id: String,
    /// Encryption key (not serialized)
    #[serde(skip)]
    pub crypto_key: Vec<u8>,
    /// Compression method to use (not serialized)
    #[serde(skip)]
    pub compression_method: CompressionMethod,
    /// Encryption method to use (not serialized)
    #[serde(skip)]
    pub encryption_method: EncryptionMethod,
    /// Whether to build MDD (resource) file (not serialized)
    #[serde(skip)]
    pub build_mdd: bool,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        BuilderConfig {
            preferred_content_block_size: 64*1024,
            preferred_key_block_size: 16*1024,
            compression_method: CompressionMethod::Deflate,
            encryption_method: EncryptionMethod::Salsa20,
            build_mdd: false,
            crypto_key: Vec::new(),
            input_path: String::new(),
            output_file: String::new(),
            register_by_email: true,
            password: String::new(),
            data_source_format: SourceType::MdictHtml,
            content_type: "Html".to_string(),
            default_sorting_locale: "root".to_string(),
            device_id: String::new(),
        }
    }
}

/// ZDB file header metadata.
///
/// Contains metadata information that goes into the ZDB file header,
/// describing the dictionary file properties and generation information.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename = "ZDB")]
pub struct ZdbHeader{
    /// Engine version that generated this file (typically "3.0")
    #[serde(rename = "@GeneratedByEngineVersion")]
    pub generated_by_engine_version: String,
    /// Minimum engine version required to read this file
    #[serde(rename = "@RequiredEngineVersion")]
    pub required_engine_version: String,
    /// Whether the dictionary uses compact format
    #[serde(rename = "@Compact")]
    pub compact: bool,
    /// Registration type (e.g., "EMail", "DeviceID")
    #[serde(rename = "@RegisterBy")]
    pub register_by: String,
    /// Date when the file was created
    #[serde(rename = "@CreationDate")]
    pub creation_date: String,
    /// Source format type code
    #[serde(rename = "@DataSourceFormat")]
    pub data_source_format: u32,
    /// CSS stylesheet for content display
    #[serde(rename = "@StyleSheet")]
    pub style_sheet: String,
    /// Unique identifier for this dictionary
    #[serde(rename = "@UUID")]
    pub uuid: String,
    /// Type of content (Html, Text, Binary)
    #[serde(rename = "@ContentType")]
    pub content_type: String,
    /// Default sorting locale
    #[serde(rename = "@DefaultSortingLocale")]
    pub default_sorting_locale: String,
}

impl ZdbHeader{
    /// Creates a ZDB header from build configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Builder configuration with dictionary settings
    ///
    /// # Returns
    ///
    /// A new ZdbHeader initialized with values from the configuration.
    pub fn from_config(config: &BuilderConfig) -> Self {
        Self {
            generated_by_engine_version: "3.0".to_string(),
            required_engine_version: "3.0".to_string(),
            compact: false,
            register_by: if config.register_by_email {"Yes".to_string()} else {"No".to_string()},
            creation_date: String::new(), // Should be the current date when generating the zdb
            data_source_format: config.data_source_format as u32,
            style_sheet: String::new(), // Not used anymore
            uuid: String::new(), // Should be calculated when generating the zdb
            content_type: config.content_type.clone(),
            default_sorting_locale: config.default_sorting_locale.clone(),
        }
    }
}

/// Main builder for ZDB dictionary files.
///
/// Orchestrates the process of building a complete ZDB file from entries,
/// managing key blocks, content blocks, indexes, and metadata.
#[derive(Debug, Clone)]
pub struct ZDBBuilder{
    /// All dictionary entries to be indexed
    pub entries: Vec<ZdbRecord>,
    /// Header metadata for the dictionary
    pub db_header: ZdbHeader,
    /// Build configuration settings
    pub config: BuilderConfig,
    /// Indexes for key blocks
    pub key_block_indexes: Vec<KeyBlockIndex>,
    /// Indexes for content blocks
    pub content_block_indexes: Vec<ContentBlockIndex>,
    /// Total size of key index data
    pub total_key_index_data_size: u64,
}

fn write_key<W:Write>(writer: &mut W, key: &[u8]) -> Result<()> {
    writer.write_u16::<BigEndian>(key.len() as u16)?; // Key length doesn't include the terminating zero
    writer.write_all(key)?;
    writer.write_u8(0)?; // Append a ending zero
    Ok(())
}

fn write_key_block_index<W:Write>(writer: &mut W, key_block_index: &KeyBlockIndex) -> Result<()> {
    writer.write_u32::<BigEndian>(key_block_index.entry_count_in_block as u32)?;
    write_key(writer, key_block_index.first_key.as_bytes())?;
    write_key(writer, key_block_index.last_key.as_bytes())?;
    writer.write_u32::<BigEndian>(key_block_index.block_length as u32)?;
    writer.write_u32::<BigEndian>(key_block_index.raw_data_length as u32)?; // Need to be calculated before writing
    Ok(())
}

impl ZDBBuilder {
    /// Creates a new ZDB builder from configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Builder configuration with all settings
    ///
    /// # Returns
    ///
    /// A new ZDBBuilder instance.
    pub fn new(config: &BuilderConfig) -> Self {
        Self {
            db_header: ZdbHeader::from_config(&config),
            config: config.clone(),
            entries: Vec::new(),
            key_block_indexes: Vec::new(),
            content_block_indexes: Vec::new(),
            total_key_index_data_size: 0,
        }
    }

    pub fn prepare_key_index(&mut self) -> Result<()> {
        //Sort data entries by collator
        let locale_id=self.config.default_sorting_locale.clone();
        //locale_id.push_str("-kc-true-kf-upper"); //Force to sort uppercase first, Just to make the display order more consistent
        let collator=UCollator::try_from(locale_id.as_str())?;
        debug!("Sorting entries by locale: {}", locale_id);
        self.entries.sort_by(
            |a, b| collator.strcoll_utf8(a.key.as_str(), b.key.as_str()).unwrap()
        );
        debug!("Sorting entries by locale: done");
        Ok(())
    }

    pub fn prepare_key_block_index_unit(&mut self, preferred_block_size: u64, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
        let mut i = 0;
        let extra_size: u64 = 1 + 8; // 1 byte ending zero + 8 bytes record offset
        let total = self.entries.len();
        let mut total_key_index_data_size: u64 = 0;
        let mut key_block_indexes = Vec::with_capacity(total/300);

        let mut progress_state = ProgressState::new("ZDBBuilder::prepare_key_block_index_unit", total as u64, 10, prog_rpt);

        while i < total {
            let mut block_size: u64 = 0;
            let mut key_block_index = KeyBlockIndex::default();
            key_block_index.first_key = self.entries[i].key.clone();
            key_block_index.first_entry_no_in_block = i as EntryNo;

            let start = i;
            while i < total {
                let key_len = self.entries[i].key.len() as u64 + extra_size;
                // 如果不是分块的第一个key，且加上这个key会超过上限，则结束本分块
                if i > start && block_size + key_len > preferred_block_size {
                    break;
                }
                block_size += key_len;
                i += 1;
            }
            key_block_index.last_key = self.entries[i - 1].key.clone();
            key_block_index.entry_count_in_block = i as u64 - key_block_index.first_entry_no_in_block as u64;
            key_block_index.block_length = block_size;
            key_block_indexes.push(key_block_index.clone());

            total_key_index_data_size += block_size;

            if progress_state.report(i as u64) {
                info!("Prepare key block index unit cancelled by user");
                return Err(ZdbError::user_interrupted());
            }
        }
        
        self.key_block_indexes = key_block_indexes;
        self.total_key_index_data_size = total_key_index_data_size;
        Ok(())
    }

    pub fn build_db_header<W: Write>(&mut self, writer: &mut W) -> Result<()> {
        self.db_header.creation_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        self.db_header.uuid = uuid::Uuid::new_v4().to_string();
        self.config.crypto_key = if self.config.password.is_empty(){
            debug!("uuid:{}",self.db_header.uuid);
            fast_hash_digest(self.db_header.uuid.as_bytes())?
        } else {
            fast_hash_digest(self.config.password.as_bytes())?
        };
        debug!("crypto_key:{}",hex::encode(&self.config.crypto_key));
        let mut header_str = serde_xml_rs::to_string(&self.db_header)?;
        remove_xml_declaration(&mut header_str);
        writer.write_u32::<BigEndian>(header_str.len() as u32 + 1)?;
        let mut header_bytes = header_str.as_bytes().to_vec();
        header_bytes.push(0);
        writer.write_all(&header_bytes)?;
        let adler = adler::adler32_slice(&header_bytes );
        writer.write_u32::<LittleEndian>(adler)?;
        Ok(())
    }
    
    pub fn build_key_block_index_unit<W: Write+Seek>(&mut self, writer: &mut W, prog_rpt: Option<ProgressReportFn>) -> Result<()> {

        if self.entries.is_empty() {
            return Err(ZdbError::invalid_parameter("No entries"));
        }
        let mut unit_builder = ZdbUnitBuilder::from_config(&self.config);

        let mut progress_state = ProgressState::new("ZDBBuilder::build_key_block_index_unit", self.key_block_indexes.len() as u64, 10, prog_rpt);
        unit_builder.write_unit_begin(writer, UnitType::KeyBlockIndex)?;        
        let mut key_block_indexes_data = Vec::<u8>::with_capacity(self.key_block_indexes.len()*100);
        for (n, key_block_index) in self.key_block_indexes.iter().enumerate() {
            write_key_block_index(&mut key_block_indexes_data, key_block_index)?;
            if progress_state.report(n as u64){
                info!("Buil key block index unit cancelled by user");
                return Err(ZdbError::user_interrupted());
            }
        }

        unit_builder.output_block(writer, &key_block_indexes_data)?;
        unit_builder.write_unit_end(writer, self.key_block_indexes.len() as u64)?;
        Ok(())
    }

    pub fn build_key_block_unit<W: Write+Seek>(&mut self, writer: &mut W, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
        let mut unit_builder = ZdbUnitBuilder::from_config(&self.config);

        let mut progress_state = ProgressState::new("ZDBBuilder::build_key_block_unit", self.key_block_indexes.len() as u64, 10, prog_rpt);
        unit_builder.write_unit_begin(writer, UnitType::Key)?;

        for (i, key_block_index) in self.key_block_indexes.iter_mut().enumerate() {
            let mut key_block_data = Vec::<u8>::with_capacity(self.config.preferred_key_block_size as usize);
            for j in 0..key_block_index.entry_count_in_block {
                let entry = &self.entries[(key_block_index.first_entry_no_in_block as u64 + j )as usize];
                key_block_data.write_u64::<BigEndian>(entry.content_offset_in_source)?;
                key_block_data.write_all(entry.key.as_bytes())?;
                key_block_data.write_u8(0)?;
            }

            if progress_state.report(i as u64) {
                info!("Buil key block unit cancelled by user");
                return Err(ZdbError::user_interrupted());
            }

            let key_block_compressed_size = unit_builder.output_block(writer, &key_block_data)?;
            key_block_index.raw_data_length = key_block_data.len() as u64;
            key_block_index.block_length = key_block_compressed_size;
        }

        unit_builder.write_unit_end(writer, self.entries.len() as u64)?;
        Ok(())
    }

    pub fn build_content_block_index_unit<W: Write+Seek>(&mut self, writer: &mut W, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
        let mut unit_builder = ZdbUnitBuilder::from_config(&self.config);
        let mut progress_state = ProgressState::new("ZDBBuilder::build_content_block_index_unit", self.content_block_indexes.len() as u64, 10, prog_rpt);
        unit_builder.write_unit_begin(writer, UnitType::ContentBlockIndex)?;
        let mut content_block_index_data = Vec::<u8>::with_capacity(self.content_block_indexes.len()*16);
        for (n, content_block_index) in self.content_block_indexes.iter().enumerate() {
            content_block_index_data.write_u64::<BigEndian>(content_block_index.block_compressed_length)?;
            content_block_index_data.write_u64::<BigEndian>(content_block_index.block_original_length)?;
            if progress_state.report(n as u64){
                info!("Buil content block index unit cancelled by user");
                return Err(ZdbError::user_interrupted());
            }
        }
        unit_builder.output_block(writer, &content_block_index_data)?;
        unit_builder.write_unit_end(writer, self.content_block_indexes.len() as u64)?;
        Ok(())
    }

    pub fn build_content_unit<W: Write+Seek, L: FnMut(&ZdbRecord) -> Result<Vec<u8>>>(&mut self, writer: &mut W, mut data_loader:L, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
        let mut progress_state = ProgressState::new("ZDBBuilder::build_content_unit", self.entries.len() as u64, 10, prog_rpt);
        let mut unit_builder = ZdbUnitBuilder::from_config(&self.config);
        unit_builder.write_unit_begin(writer, UnitType::Content)?;
        self.content_block_indexes.clear();
        let mut offset_in_source = 0;
        let mut offset_in_unit = 0;
        let total_entries = self.entries.len();
        let mut content_data = Vec::<u8>::with_capacity(self.config.preferred_content_block_size as usize);

        let mut i = 0;
        let mut content_offset_in_source = 0;
        while  i < total_entries {
            content_data.clear();
            while i < total_entries {
                let entry = &mut self.entries[i];
                let content = data_loader(entry)?;
                entry.content_offset_in_source = content_offset_in_source;
                content_offset_in_source += content.len() as u64;
                content_data.extend(content);
                i += 1;
                //Because we don't know the real content length before loading it. 
                //So we need to break the loop when the content data length is greater than the preferred block size.
                if content_data.len() > self.config.preferred_content_block_size as usize {
                    break;
                }   
            }

            let data_block_size= unit_builder.output_block(writer, &content_data)?;

            if progress_state.report(i as u64) {
                info!("Buil content unit cancelled by user");
                return Err(ZdbError::user_interrupted());
            }
    
            let content_block_index = ContentBlockIndex{
                block_offset_in_source: offset_in_source,
                block_offset_in_unit: offset_in_unit,
                block_original_length: content_data.len() as u64,
                block_compressed_length: data_block_size,
            };
            self.content_block_indexes.push(content_block_index);
            offset_in_source += content_data.len() as u64;
            offset_in_unit += data_block_size as u64;
        }
        unit_builder.write_unit_end(writer, self.entries.len() as u64)?;
        Ok(())
    }

    /// Build ZDB with a specific data loader
    fn build_with_data_loader<T: crate::builder::data_loader::DataLoader>(
        mut zdb_builder: ZDBBuilder,
        mut zdb_writer: std::io::BufWriter<std::fs::File>,
        mut data_loader: T,
        entry_records: Vec<ZdbRecord>,
        prog_rpt: Option<ProgressReportFn>
    ) -> Result<()> {
        // Load entries from data loader
        zdb_builder.entries = entry_records;

        info!("Sorting index...");
        zdb_builder.prepare_key_index()?;
        info!("done");

        info!("Preparing key index...");
        zdb_builder.prepare_key_block_index_unit(zdb_builder.config.preferred_key_block_size as u64, prog_rpt)?;
        info!("done");

        info!("Building content unit...");
        // Use closure to pass DataLoader::load_data to build_content_unit
        zdb_builder.build_content_unit(
            &mut zdb_writer,
            |entry| data_loader.load_data(entry),
            prog_rpt,
        )?;
        info!("done");

        info!("Building content block index unit...");
        zdb_builder.build_content_block_index_unit(&mut zdb_writer, prog_rpt)?;
        info!("done");

        info!("Building key block unit...");
        zdb_builder.build_key_block_unit(&mut zdb_writer, prog_rpt)?;
        info!("done");

        info!("Building key block index unit...");
        zdb_builder.build_key_block_index_unit(&mut zdb_writer, prog_rpt)?;
        info!("done");

        info!("Build completed");

        Ok(())
    }

    /// Build ZDB file from configured data source
    ///
    /// This is the main entry point for building a ZDB dictionary file.
    /// It handles the entire process from loading source data to writing the output file.
    ///
    /// # Arguments
    ///
    /// * `config` - Build configuration specifying input/output paths and settings
    /// * `prog_rpt` - Optional progress reporter callback function
    ///   - Called periodically during the build process
    ///   - Return `true` from the callback to cancel the build
    ///   - Return `false` to continue building
    ///
    /// # Supported Source Formats
    ///
    /// - `MdictHtml`: MDX dictionary files with HTML content
    /// - `Zdb`: Existing ZDB files (for conversion/optimization)
    /// - `Directory`: Directory structure with individual entry files
    /// - `StarDict`, `Kdic`, `SGD`: Other dictionary formats
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mdx::builder::{ZDBBuilder, BuilderConfig, SourceType};
    ///
    /// # fn main() -> mdx::Result<()> {
    /// let mut config = BuilderConfig::default();
    /// config.input_path = "dictionary.mdx".to_string();
    /// config.output_file = "output.zdb".to_string();
    /// config.data_source_format = SourceType::MdictHtml;
    ///
    /// // Build without progress reporting
    /// ZDBBuilder::build_with_config(&config, None)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # With Progress Reporting
    ///
    /// ```no_run
    /// use mdx::builder::{ZDBBuilder, BuilderConfig};
    ///
    /// # fn main() -> mdx::Result<()> {
    /// fn progress_callback(state: &mut mdx::utils::progress_report::ProgressState) -> bool {
    ///     println!("Progress: {}/{}", state.current, state.total);
    ///     false // Continue building
    /// }
    ///
    /// let config = BuilderConfig::default();
    /// ZDBBuilder::build_with_config(&config, Some(progress_callback))?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Input file cannot be read
    /// - Output file cannot be created
    /// - Source format is not supported
    /// - Data corruption is detected
    /// - Compression/encryption fails
    pub fn build_with_config(config: &BuilderConfig, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
        use std::fs::File;
        use std::io::BufWriter;
        
        let mut zdb_builder = ZDBBuilder::new(config);
        let mut zdb_writer = BufWriter::new(File::create(&zdb_builder.config.output_file)?);
        zdb_builder.build_db_header(&mut zdb_writer)?;

        info!("Loading source: {}...", config.input_path);


        // Create appropriate data loader based on SourceType and build
        match config.data_source_format {
            SourceType::MdictHtml => {
                use crate::builder::mdict_source_loader::MDictSourceLoader;
                let (data_loader, entry_records) = MDictSourceLoader::new(&config.input_path, prog_rpt)?;
                Self::build_with_data_loader(zdb_builder, zdb_writer, data_loader, entry_records, prog_rpt)
            },
            SourceType::Zdb => {
                use crate::builder::zdb_loader::ZdbLoader;
                let (data_loader, entry_records) = ZdbLoader::new(&config.input_path, &config.device_id, &config.password, prog_rpt)?;
                
                // Update sorting locale if empty and source is ZDB
                if zdb_builder.config.default_sorting_locale.is_empty() {
                    zdb_builder.config.default_sorting_locale = 
                        data_loader.input_reader.meta.db_info.locale_id.clone();
                }
                
                Self::build_with_data_loader(zdb_builder, zdb_writer, data_loader, entry_records, prog_rpt)
            },
            SourceType::Directory => {
                use crate::builder::data_dir_loader::DataDirLoader;
                let (data_loader, entry_records) = DataDirLoader::new(&config.input_path, prog_rpt)?;
                Self::build_with_data_loader(zdb_builder, zdb_writer, data_loader, entry_records, prog_rpt)
            },
            _ => {
                Err(ZdbError::invalid_data_format(format!("Unsupported source format: {:?}", config.data_source_format)))
            }
        }
    }
}
