//! Key block and key index structures for dictionary lookups.
//!
//! This module provides the core data structures for dictionary key management:
//! - [`KeyIndex`]: Represents a single dictionary key with its metadata
//! - [`KeyBlock`]: A block of key indexes for efficient lookup
//! - Entry number types and constants for key referencing
//!
//! # Examples
//!
//! ```no_run
//! use mdx::key_block::{KeyIndex, INVALID_ENTRY_NO};
//!
//! let key_index = KeyIndex {
//!     key: "hello".to_string(),
//!     key_raw: b"hello".to_vec(),
//!     sort_key: vec![],
//!     content_offset_in_source: 0,
//!     entry_no: 0,
//! };
//! ```

use std::cmp::Ordering;
use std::io::{Cursor, Read, Seek};
use std::rc::Rc;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};

use super::key_block_index::KeyBlockIndex;
use crate::storage::meta_unit::{MetaUnit, ZdbVersion};
use crate::storage::reader_helper::decode_bytes_to_string;
use crate::utils::sort_key::get_sort_key;
use crate::storage::storage_block::StorageBlock;
use crate::utils::{binary_search_first, locale_compare, sort_key_compare, KeyComparable, RandomAccessable};
use crate::{Result, ZdbError};

/// Type alias for dictionary entry numbers.
pub type EntryNo = i64;

/// Constant representing an invalid entry number.
pub const INVALID_ENTRY_NO: EntryNo = -1;

/// Constant representing a union entry number (for merged entries).
pub const UNION_ENTRY_NO: EntryNo = -2;

/// Represents a single dictionary key with its associated metadata.
///
/// This structure contains all information needed to locate and retrieve
/// a dictionary entry's content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyIndex {
    /// The dictionary key as a UTF-8 string
    pub key: String,
    /// The raw bytes of the key (may be UTF-16 or UTF-8)
    pub key_raw: Vec<u8>,
    /// The sort key used for locale-aware ordering
    pub sort_key: Vec<u8>,
    /// Offset of the content in the content data section
    pub content_offset_in_source: u64,
    /// Entry number for this key
    pub entry_no: EntryNo,
}

impl Default for KeyIndex {
    fn default() -> Self {
        Self {
            key: String::default(),
            key_raw: Vec::default(),
            sort_key: Vec::default(),
            content_offset_in_source: u64::default(),
            entry_no: INVALID_ENTRY_NO,
        }
    }
}

impl KeyComparable for KeyIndex {    
    fn compare_with(&self, other:&str, other_sort_key:&[u8], start_with: bool, meta_info: &MetaUnit) -> Result<Ordering> {
        if meta_info.is_v3(){
            locale_compare(&self.key, other, start_with, meta_info)
        }else{
            sort_key_compare(&self.sort_key,&other_sort_key, start_with)
        }
    }
}

/// A block of key indexes for efficient dictionary lookups.
///
/// Key blocks group multiple key indexes together to reduce memory usage
/// and improve lookup performance through binary search.
#[derive(Debug, Clone)]
pub struct KeyBlock {
    /// Index information for this key block
    pub key_block_index: KeyBlockIndex,
    /// The key indexes contained in this block
    pub key_indexes: Vec<KeyIndex>,
    /// Metadata about the dictionary
    pub meta_info: Rc<MetaUnit>,
}

impl RandomAccessable<KeyIndex> for KeyBlock{
    fn get_item(&self, index:usize)->Result<&KeyIndex> {
        return Ok(&self.key_indexes[index]);
    }
    fn len(&self)->usize {
        return self.key_indexes.len();
    }
}

fn key_str_from_cursor(cursor: &mut Cursor<&Vec<u8>>,meta_info: &MetaUnit) -> Result<(String, Vec<u8>)> {
    let start_pos=cursor.position();
    let mut end_pos=start_pos;
    while cursor.position()<cursor.get_ref().len() as u64 {
        if meta_info.db_info.is_utf16 {
            if cursor.read_u16::<LittleEndian>()?==0 {
                end_pos=cursor.position()-2;
                break;
            }
        }else{
            if cursor.read_u8()?==0 {
                end_pos=cursor.position()-1;
                break;
            }   
        }
    }
    let key_bytes= &cursor.get_ref()[start_pos as usize..end_pos as usize];
    return Ok((decode_bytes_to_string(key_bytes, &meta_info.encoding_obj)?, key_bytes.to_vec()));
}

impl KeyBlock {

    //TODO it's very time consuming to get sort_key for each key, need to optimize it
    pub fn from_reader<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>, key_block_index: &KeyBlockIndex) -> Result<Self> {
        let block_data = match meta_info.version {
            ZdbVersion::V3 => StorageBlock::from_reader_v3(reader, &meta_info)?,
            ZdbVersion::V2 | ZdbVersion::V1 => StorageBlock::from_reader_v1_v2(reader, &meta_info, &meta_info.crypto_key, key_block_index.block_length as u32, key_block_index.raw_data_length as u32)?,
        };
        let mut key_indexes = Vec::with_capacity(key_block_index.entry_count_in_block as usize);
        let mut cursor = Cursor::new(&block_data.data);
        for i in 0..key_block_index.entry_count_in_block {
            let content_offset_in_source = match meta_info.version {    
                ZdbVersion::V3|ZdbVersion::V2 => cursor.read_u64::<BigEndian>()?,
                ZdbVersion::V1=>cursor.read_u32::<BigEndian>()? as u64,
            };
            let (key, key_raw) = key_str_from_cursor(&mut cursor, &meta_info)?;
            let sort_key = get_sort_key(&key_raw, &meta_info)?;
            let key_index=KeyIndex{
                key, key_raw,
                content_offset_in_source,
                entry_no : i as EntryNo + key_block_index.first_entry_no_in_block,
                sort_key,
            };
            key_indexes.push(key_index);
        }
        Ok(Self { key_indexes, key_block_index: key_block_index.clone(), meta_info: meta_info.clone() })
    }

    pub fn find_index(&self, key: &str, prefix_match: bool, partial_match: bool) -> Result<Option<KeyIndex>> {
        let meta_info = self.meta_info.clone();
        binary_search_first(self, key, &meta_info, prefix_match, partial_match)
    }

    pub fn get_index(&self, entry_no: EntryNo) -> Result<KeyIndex> {
        if entry_no < self.key_block_index.first_entry_no_in_block
            || entry_no >= self.key_block_index.first_entry_no_in_block + self.key_block_index.entry_count_in_block as EntryNo {
            return Err(ZdbError::invalid_parameter("entry_no is out of range"));
        }
        let index = entry_no - self.key_block_index.first_entry_no_in_block;
        self.key_indexes
            .get(index as usize)
            .cloned() // Option<&KeyIndex> -> Option<KeyIndex>
            .ok_or_else(|| ZdbError::invalid_parameter("entry_no is out of range"))
    }
}