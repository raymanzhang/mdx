//! Content blocks for storing dictionary entry data.
//!
//! This module handles individual content blocks within ZDB files,
//! which store the actual dictionary entry content (definitions, translations, etc.).
//! Content blocks can be compressed and/or encrypted.

use std::io::{Read, Seek};

use encoding_rs::Encoding;

use super::content_block_index_unit::ContentBlockIndex;
use crate::storage::meta_unit::{MetaUnit, ZdbVersion};
use crate::storage::reader_helper::decode_bytes_to_string;
use super::storage_block::StorageBlock;

/// A content block from a ZDB file.
///
/// Contains the actual dictionary entry data, indexed by position in the source.
pub struct ContentBlock {
    pub block_index: ContentBlockIndex,
    pub block: Vec<u8>,
}

impl ContentBlock {
    /// Reads a content block from a reader.
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to read from
    /// * `meta_info` - Dictionary metadata
    /// * `block_index` - Index information for the block
    pub fn from_reader<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit, block_index: &ContentBlockIndex) -> crate::Result<Self> {
        let block_data=
            match meta_info.version {
                ZdbVersion::V1|ZdbVersion::V2=>
                    StorageBlock::from_reader_v1_v2(
                        reader, 
                        meta_info, 
                        &meta_info.crypto_key,
                        block_index.block_compressed_length as u32,
                        block_index.block_original_length as u32
                    )?,
                ZdbVersion::V3=>StorageBlock::from_reader_v3(reader, meta_info)?,
            };
        Ok(Self { block_index: block_index.clone(), block: block_data.data })
    }

    /// Gets a slice of content from this block.
    ///
    /// # Arguments
    ///
    /// * `offset` - Offset within the block
    /// * `length` - Number of bytes to read
    pub fn get_content_as_slice(&self, offset: u64, length: u64) -> crate::Result<&[u8]> {
        if offset < self.block_index.block_offset_in_source || offset + length > self.block_index.block_offset_in_source + self.block_index.block_original_length {
            return Err(crate::ZdbError::invalid_parameter(format!("offset out of range: offset={}, length={}, block_offset_in_source={}, block_original_length={}", offset, length, self.block_index.block_offset_in_source, self.block_index.block_original_length)));
        }
        let block_offset = offset - self.block_index.block_offset_in_source ;
        Ok(&self.block[block_offset as usize..(block_offset + length) as usize])
    }

    /// Gets content bytes from this block.
    pub fn get_bytes(&self, offset: u64, length: u64) -> crate::Result<Vec<u8>> {
        let content = self.get_content_as_slice(offset, length)?;
        Ok(content.to_vec())
    }

    /// Gets content as a decoded string from this block.
    pub fn get_string(&self, offset: u64, length: u64, encoding_obj: &'static Encoding) -> crate::Result<String> {
        let content = self.get_content_as_slice(offset, length)?;
        decode_bytes_to_string(content, encoding_obj)
    }
}