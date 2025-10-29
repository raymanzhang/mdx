//! Content unit handling for accessing dictionary record data.
//!
//! This module manages the content unit of a ZDB file, which is responsible for
//! organizing and providing access to all dictionary records.

use std::io::{Read, Seek, SeekFrom};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::content_block::ContentBlock;
use super::content_block_index_unit::{ContentBlockIndex, ContentBlockIndexUnit};
use crate::storage::meta_unit::MetaUnit;
use crate::storage::unit_base::{read_data_info_section, UnitInfoSection};
use crate::Result;

/// Metadata for dictionary record content.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename = "RecordData")]
pub struct ContentDataInfo {
    #[serde(rename = "@encoding")]
    pub encoding: String,
    #[serde(rename = "@recordCount")]
    pub record_count: u64,
    
}
// <RecordData encoding="utf-8" recordCount="123" />

/// Content unit for accessing all dictionary records.
///
/// Manages the organization of content blocks and provides methods
/// for retrieving individual content blocks.
pub struct ContentUnit {
    pub total_record_count: u64,
    pub content_data_offset_in_file: u64,
    pub meta_info: Rc<MetaUnit>,
    pub block_count: u32,
}

impl ContentUnit {
    /// Gets a specific content block from the reader.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to read from
    /// * `content_block_index` - Index information for the block
    pub fn get_content_block<R: Read+Seek>(&self, reader: &mut R, content_block_index: &ContentBlockIndex) -> Result<ContentBlock> {
        reader.seek(SeekFrom::Start(content_block_index.block_offset_in_unit + self.content_data_offset_in_file))?;
        let block = ContentBlock::from_reader(reader, &self.meta_info, content_block_index)?;
        Ok(block)
    }
}

impl ContentUnit {
    /// Loads content unit from V3 format reader.
    pub fn from_reader_v3<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>) -> crate::Result<Self> {
        let info = UnitInfoSection::from_reader(reader)?;
        let content_data_offset = reader.seek(SeekFrom::Current(0))?;
        // Skip to the end of data section
        reader.seek(SeekFrom::Current(info.data_section_length as i64))?;
        let data_info = read_data_info_section::<ContentDataInfo, R>(reader, &meta_info)?;
        let record_count = data_info.record_count;
        Ok(Self { total_record_count: record_count, content_data_offset_in_file: content_data_offset, meta_info: meta_info.clone(), block_count: info.block_count })
    }   

    /// Loads content unit from V1/V2 format reader.
    pub fn from_reader_v1_v2<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>, content_block_index_unit: &ContentBlockIndexUnit) -> Result<Self> {
        let content_data_offset_in_file = reader.seek(SeekFrom::Current(0))?;
        let record_count = content_block_index_unit.record_count;
        Ok(Self { total_record_count: record_count, content_data_offset_in_file, meta_info: meta_info.clone(), block_count: content_block_index_unit.block_index_entries.len() as u32 })
    }
}