//! ZDB unit builder for constructing individual units in a ZDB file.
//!
//! This module provides the [`ZdbUnitBuilder`] struct which handles the low-level
//! construction of ZDB file units (key blocks, content blocks, and their indexes).
//! It manages block writing, compression, encryption, and metadata tracking.

use std::io::{Seek, SeekFrom, Write};

use crate::builder::zdb_builder::BuilderConfig;
use crate::storage::content_block_index_unit::ContentBlockIndexDataInfo;
use crate::storage::content_unit::ContentDataInfo;
use crate::storage::key_block_index_unit::KeyBlockIndexDataInfo;
use crate::storage::key_unit::KeyDataInfo;
use crate::storage::storage_block::StorageBlock;
use crate::storage::unit_base::{write_data_info_section, UnitInfoSection, UnitType};
use crate::Result;

/// Builder for constructing individual units in a ZDB file.
///
/// This struct handles the low-level writing of ZDB file units, including:
/// - Key block indexes and key blocks
/// - Content block indexes and content blocks
/// - Compression and encryption of block data
/// - Metadata tracking and unit info sections
pub struct ZdbUnitBuilder {
    /// Configuration for compression, encryption, and other build settings
    pub config: BuilderConfig,
    /// Unit information section containing metadata about the current unit
    pub unit_info: UnitInfoSection, 
    /// File position where the unit info section was written
    pub unit_info_pos: u64,
}

impl ZdbUnitBuilder {
    /// Creates a new unit builder from the given configuration.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mdx::builder::{ZdbUnitBuilder, BuilderConfig};
    ///
    /// let config = BuilderConfig::default();
    /// let builder = ZdbUnitBuilder::from_config(&config);
    /// ```
    pub fn from_config(config: &BuilderConfig) -> Self {
        Self {
            config: config.clone(),
            unit_info: UnitInfoSection::default(),
            unit_info_pos: 0,
        }
    }

    /// Begins writing a new unit to the writer.
    ///
    /// This writes the unit info section header and records its position
    /// so it can be updated later with the correct metadata.
    ///
    /// # Arguments
    ///
    /// * `writer` - The writer to write the unit to
    /// * `unit_type` - The type of unit being written (Key, Content, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if writing to the writer fails.
    pub fn write_unit_begin<W: Write+Seek>(&mut self, writer: &mut W, unit_type: UnitType) -> Result<()> {
        self.unit_info.unit_type = unit_type;
        self.unit_info_pos = writer.seek(SeekFrom::Current(0))?;
        self.unit_info.to_writer(writer)?;
        Ok(())
    }

    /// Writes a data block to the writer with compression and encryption.
    ///
    /// This method compresses and encrypts the block data according to the
    /// builder configuration, then writes it to the writer. It updates the
    /// unit info section with the block count and data lengths.
    ///
    /// # Arguments
    ///
    /// * `writer` - The writer to write the block to
    /// * `block_data` - The raw block data to compress and encrypt
    ///
    /// # Returns
    ///
    /// Returns the number of bytes written (after compression/encryption).
    ///
    /// # Errors
    ///
    /// Returns an error if compression, encryption, or writing fails.
    pub fn output_block<W: Write+Seek>(&mut self, writer: &mut W, block_data: &[u8]) -> Result<u64> {
        let block_data_len = StorageBlock::to_writer(writer, &block_data, &self.config.crypto_key, self.config.compression_method, self.config.encryption_method)?;
        self.unit_info.block_count += 1;
        self.unit_info.data_section_length += block_data_len as u64;
        self.unit_info.orig_data_section_length += block_data.len() as u64;
        Ok(block_data_len)
    }

    /// Finalizes the unit by writing the data info section.
    ///
    /// This method:
    /// 1. Seeks back to the unit info position and rewrites it with correct metadata
    /// 2. Seeks to the end of the unit data
    /// 3. Writes the appropriate data info section based on the unit type
    ///
    /// # Arguments
    ///
    /// * `writer` - The writer to finalize the unit in
    /// * `count` - The count of items in the unit (keys, records, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if seeking or writing fails.
    pub fn write_unit_end<W: Write+Seek>(&mut self, writer: &mut W, count: u64) -> Result<()> {
        // Rewrite unit info with correct data
        let data_info_pos = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(self.unit_info_pos))?;
        self.unit_info.to_writer(writer)?; 
        writer.seek(SeekFrom::Start(data_info_pos))?;
        let encoding = "utf-8".to_string();
        match self.unit_info.unit_type {
            UnitType::KeyBlockIndex => {
                let data_info = KeyBlockIndexDataInfo{
                    block_count: count as u32,
                    encoding: encoding,
                    locale_id: self.config.default_sorting_locale.clone(),
                };
                write_data_info_section(writer, &data_info, &self.config.crypto_key, self.config.compression_method, self.config.encryption_method)?;
            }
            UnitType::Key => {
                let data_info = KeyDataInfo{
                    key_count: count,
                    encoding: encoding,
                    locale_id: self.config.default_sorting_locale.clone(),
                };
                write_data_info_section(writer, &data_info, &self.config.crypto_key, self.config.compression_method, self.config.encryption_method)?;
            }
            UnitType::ContentBlockIndex => {
                let data_info = ContentBlockIndexDataInfo{
                    record_count: count,
                    encoding: encoding,
                };
                write_data_info_section(writer, &data_info, &self.config.crypto_key, self.config.compression_method, self.config.encryption_method)?;
            }
            UnitType::Content => {
                let data_info = ContentDataInfo{
                    record_count: count,
                    encoding: encoding,
                };
                write_data_info_section(writer, &data_info, &self.config.crypto_key, self.config.compression_method, self.config.encryption_method)?;
            }
            _ => {}
        }
        Ok(())
    }
}
