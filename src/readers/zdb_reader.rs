//! Low-level ZDB file reader for dictionary data.
//!
//! This module provides the core reading functionality for ZDB (MDict/MDD) files.
//! It handles:
//! - File parsing and metadata extraction
//! - Key and content block index reading
//! - Entry lookup by key or entry number
//! - Content retrieval with decompression and decryption
//! - Link handling (@@@LINK= support)
//! - Block caching for performance
//!
//! This module works with all ZDB versions (V1, V2, V3).

use std::cmp::{min, Ordering};
use std::collections::{HashSet, LinkedList};
use std::io::{BufReader, Read, Seek};
use std::num::NonZeroUsize;
use std::path::Path;
use std::rc::Rc;
use std::str;

use lru::LruCache;

use crate::storage::content_block::ContentBlock;
use crate::storage::content_block_index_unit::ContentBlockIndexUnit;
use crate::storage::content_unit::ContentUnit;
use crate::storage::key_block::{EntryNo, KeyIndex};
use crate::storage::key_block_index_unit::KeyBlockIndexUnit;
use crate::storage::key_unit::KeyUnit;
use crate::storage::meta_unit::{ContentType, MetaUnit};
use crate::storage::reader_helper::decode_bytes_to_string;
use crate::utils::sort_key::get_sort_key;
use crate::utils::KeyComparable;
use crate::{Result, ZdbError};

const LINK_PREFIX: &[u8] = b"@@@LINK=";
const LINK_PREFIX_W: &[u8] = &[
    0x40, 0x00, // '@' (U+0040)
    0x40, 0x00, // '@' (U+0040)
    0x40, 0x00, // '@' (U+0040)
    0x4C, 0x00, // 'L' (U+004C)
    0x49, 0x00, // 'I' (U+0049)
    0x4E, 0x00, // 'N' (U+004E)
    0x4B, 0x00, // 'K' (U+004B)
    0x3D, 0x00, // '=' (U+003D)
];

/// Low-level ZDB dictionary reader.
///
/// This struct provides direct access to ZDB file contents including key indexes,
/// content blocks, and metadata. It includes built-in caching for performance.
pub struct ZdbReader<R: Read + Seek> {
    pub meta: Rc<MetaUnit>,
    content: ContentUnit,
    content_block_index: ContentBlockIndexUnit,
    key_blocks: KeyUnit,
    key_block_indexes: KeyBlockIndexUnit,
    reader: R,
    block_cache: LruCache<u64, Rc<ContentBlock>>,
}

impl<R: Read + Seek> ZdbReader<R> {
    /// Opens a ZDB file from a file path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the ZDB file
    /// * `device_id` - Device identifier for license verification
    /// * `license_data` - License key data
    ///
    /// # Returns
    ///
    /// Returns an initialized ZdbReader on success.
    pub fn from_file<P: AsRef<Path>>(
        path: P,
        device_id: &str,
        license_data: &str,
    ) -> Result<ZdbReader<BufReader<std::fs::File>>> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        ZdbReader::from_reader(reader, device_id, license_data)
    }

    /// Opens a ZDB file from a generic reader.
    ///
    /// # Arguments
    ///
    /// * `reader` - A reader for the ZDB file data
    /// * `device_id` - Device identifier for license verification
    /// * `license_data` - License key data
    ///
    /// # Returns
    ///
    /// Returns an initialized ZdbReader on success.
    pub fn from_reader(reader: R, device_id: &str, license_data: &str) -> Result<ZdbReader<R>> {
        let mut reader = reader;
        // First create a temporary MetaUnit with content_data_total_length = 0
        let temp_meta = MetaUnit::from_reader(&mut reader, device_id, license_data, 0)?;
        if temp_meta.is_v3(){
            return ZdbReader::from_reader_v3(reader, temp_meta);
        }else{
            return ZdbReader::from_reader_v1_v2(reader, temp_meta);
        }
    }

    /// Loads ZDB file from V1/V2 format.
    pub fn from_reader_v1_v2(mut reader: R, meta: MetaUnit) -> Result<ZdbReader<R>> {
        let rc_meta = Rc::new(meta);
        let key_block_indexes =  KeyBlockIndexUnit::from_reader_v1_v2(&mut reader, &rc_meta)?;
        let key_blocks = KeyUnit::from_reader_v1_v2(&mut reader, &rc_meta, &key_block_indexes)?;
        let content_block_indexes = ContentBlockIndexUnit::from_reader_v1_v2(&mut reader, &rc_meta)?;
        let content = ContentUnit::from_reader_v1_v2(&mut reader, &rc_meta, &content_block_indexes)?;

        // Create a new MetaUnit with the correct content_data_total_length
        let mut updated_meta = (*rc_meta).clone();
        updated_meta.content_data_total_length = content_block_indexes.total_original_data_length;
        let rc_meta = Rc::new(updated_meta);

        Ok(ZdbReader {
            meta: rc_meta,
            content,
            content_block_index: content_block_indexes,
            key_blocks,
            key_block_indexes,
            reader,
            block_cache: LruCache::new(NonZeroUsize::new(10).unwrap()),
        })
    }

    /// Loads ZDB file from V3 format.
    pub fn from_reader_v3(mut reader: R, meta: MetaUnit) -> Result<ZdbReader<R>> {
        let rc_meta = Rc::new(meta);
        let content = ContentUnit::from_reader_v3(&mut reader, &rc_meta)?;
        let content_block_index = ContentBlockIndexUnit::from_reader_v3(&mut reader, &rc_meta, content.block_count)?;
        
        // Create a new MetaUnit with the correct content_data_total_length
        let mut updated_meta = (*rc_meta).clone();
        updated_meta.content_data_total_length = content_block_index.total_original_data_length;
        let rc_meta = Rc::new(updated_meta);

        let entry_keys = KeyUnit::from_reader_v3(&mut reader, &rc_meta)?;
        let key_block_index = KeyBlockIndexUnit::from_reader_v3(&mut reader, &rc_meta)?;

        if content.total_record_count != key_block_index.total_key_count
            || entry_keys.total_key_count != content.total_record_count
        {
            return Err(ZdbError::invalid_data_format("Record count mismatch"));
        }

        Ok(ZdbReader {
            meta: rc_meta,
            content,
            content_block_index,
            key_blocks: entry_keys,
            key_block_indexes: key_block_index,
            reader,
            block_cache: LruCache::new(NonZeroUsize::new(10).unwrap()),
        })
    }

    pub fn get_entry_count(&self) -> u64 {
        self.content.total_record_count
    }

    pub fn find_first_match(
        &mut self,
        key: &str,
        prefix_match: bool,
        partial_match: bool,
        best_match: bool,
    ) -> crate::Result<Option<KeyIndex>> {
        let key_block_index =
            self.key_block_indexes
                .find_index(key, prefix_match, partial_match)?;
        if let Some(key_block_index) = key_block_index {
            let key_block = self.key_blocks.get_key_block(&mut self.reader, &key_block_index)?;
            let key_index = key_block.borrow().find_index(
                key,
                prefix_match,
                partial_match,
            )?;
            if let Some(key_index) = key_index {
                if best_match && key_index.key!=key{
                    let sort_key = get_sort_key(key.as_bytes(), &self.meta)?;
                    for i in key_index.entry_no+1..self.get_entry_count() as EntryNo{
                        let index = self.get_index(i)?;
                        if key==index.key{ //If this index is the same as the key, return it
                            return Ok(Some(index));
                        }else if index.compare_with(&key, &sort_key, false, &self.meta)? != Ordering::Equal {
                            break;
                        }
                    }
                }
                return Ok(Some(key_index));
            }
        }
        return Ok(None);
    }

    pub fn get_similar_indexes(
        &mut self,
        key_index: &KeyIndex,
        start_with: bool,
        max_count: u64,
    ) -> crate::Result<LinkedList<KeyIndex>> {
        let mut key_indexes = LinkedList::new();
        key_indexes.push_back(key_index.clone());
        let max_count = min(max_count, self.get_entry_count() - key_index.entry_no as u64);
        let search_sort_key = get_sort_key(key_index.key.as_bytes(), &self.meta)?;
        for i in 1..max_count {
            let index = self.get_index(key_index.entry_no + i as EntryNo)?;
            if index.compare_with(&key_index.key, &search_sort_key, start_with, &self.meta)? == Ordering::Equal {
                key_indexes.push_back(index);
            } else {
                break;
            }
        }
        Ok(key_indexes)
    }

    pub fn get_content_length(&mut self, entry_no: EntryNo) -> crate::Result<u64> {
        let offset1 = self.get_index(entry_no)?.content_offset_in_source;
        let offset2 = if entry_no < self.key_block_indexes.total_key_count as EntryNo - 1 {
            self.get_index(entry_no + 1)?.content_offset_in_source
        } else {
            self.meta.content_data_total_length
        };
        Ok(offset2 - offset1)
    }

    pub fn get_content_block(&mut self, key_index: &KeyIndex) -> crate::Result<Rc<ContentBlock>> {
        let content_block_index = self
            .content_block_index
            .get_index(key_index.content_offset_in_source)?;
        let content_block = if let Some(block) = self
            .block_cache
            .peek(&content_block_index.block_offset_in_unit)
        {
            Rc::clone(&block)
        } else {
            // 读取数据块
            let block = Rc::new(self.content.get_content_block(
                &mut self.reader,
                &content_block_index,
            )?);
            self.block_cache
                .put(content_block_index.block_offset_in_unit, block.clone());
            block
        };
        Ok(content_block)
    }

    fn resolve_link_target_with_visited(&mut self, start_index: &KeyIndex, visited: Option<&mut HashSet<u64>>) -> crate::Result<KeyIndex> {
        //TODO: this function will try to load the content of the target entry, but the content is not used if it's not a link.
        //It can be optimized by returning the content of the target entry if it's not a link. Or don't try to check if it's a link 
        //if the entry's content length is larger than a certain threshold.
        let mut owned_visited: HashSet<u64>;
        let visited_ref: &mut HashSet<u64> = match visited {
            Some(v) => v,
            None => {
                owned_visited = HashSet::new();
                &mut owned_visited
            }
        };
        let mut current = start_index.clone();
        loop {
            if !visited_ref.insert(current.entry_no as u64) {
                let mut visited_str = String::new();
                for entry_no in visited_ref.iter() {
                    let index = self.get_index(*entry_no as EntryNo)?;
                    visited_str.push_str(&format!("{}: {}\n", index.entry_no, index.key));
                }
                visited_str.push_str(&format!("{}: {}\n", current.entry_no, current.key));
                return Err(ZdbError::invalid_data_format(format!("Cyclic link detected, entry links:\n{}", visited_str) ));
            }

            //zdb's content type could be binary, so we need to decode it to string first
            let bin_content = self.get_data(&current, false)?;
            
            if bin_content.starts_with(LINK_PREFIX) || bin_content.starts_with(LINK_PREFIX_W) {
                let content = decode_bytes_to_string(&bin_content, &self.content.meta_info.encoding_obj)?;

                let target_entry_key = content[LINK_PREFIX.len()..].trim_end();
                let target_entry_index = self.find_first_match(target_entry_key, false, false, true)?;
                if let Some(target_entry_index) = target_entry_index {
                    if current.entry_no == target_entry_index.entry_no {
                        return Err(ZdbError::invalid_data_format(format!("Link to self, entry:{}, target:{}", current.key, target_entry_key)));
                    }
                    current = target_entry_index;
                    continue;
                } else {
                    return Err(ZdbError::invalid_data_format(format!("Can't resolve link target: {}", target_entry_key)));
                }
            }
            return Ok(current);
        }
    }

    pub fn get_data_by_key(&mut self, key: &str) -> crate::Result<Option<Vec<u8>>> {
        let key_index = self.find_first_match(key, false, false, true)?;
        if let Some(key_index) = key_index {
            return Ok(Some(self.get_data(&key_index, true)?))
        } else {
            return Ok(None);
        }
    }

    pub fn get_data(&mut self, key_index: &KeyIndex, resolve_link: bool) -> crate::Result<Vec<u8>> {
        let resolved_index = if resolve_link {
            self.resolve_link_target_with_visited(key_index, None)?
        } else {
            key_index.clone()
        };
        let content_block = self.get_content_block(&resolved_index)?;
        let content = content_block.get_content_as_slice(
            resolved_index.content_offset_in_source,
            self.get_content_length(resolved_index.entry_no)?,
        )?;
        Ok(content.to_vec())
    }

    pub fn get_string(&mut self, key_index: &KeyIndex, resolve_link: bool) -> crate::Result<String> {
        let resolved_index = if resolve_link {
            self.resolve_link_target_with_visited(key_index, None)?
        } else {
            key_index.clone()
        };
        let content_block = self.get_content_block(&resolved_index)?;
        content_block.get_string(
            resolved_index.content_offset_in_source,
            self.get_content_length(resolved_index.entry_no)?,
            &self.content.meta_info.encoding_obj
        )
    }

    pub fn get_index(&mut self, entry_no: EntryNo) -> crate::Result<KeyIndex> {
        let key_block_index = self.key_block_indexes.get_index(entry_no)?;
        let key_block =
            self.key_blocks
                .get_key_block(&mut self.reader, key_block_index)?;
        let key_index = key_block.borrow().get_index(entry_no)?;
        Ok(key_index)
    }

    pub fn get_indexes(&mut self, start_entry_no: EntryNo, max_count: u64) -> crate::Result<LinkedList<KeyIndex>> {
        if start_entry_no >= self.get_entry_count() as EntryNo {
            return Ok(LinkedList::new());
        }
        let mut indexes = LinkedList::new();
        let end_entry_no = min (start_entry_no + max_count as EntryNo, self.get_entry_count() as EntryNo);
        for i in start_entry_no..end_entry_no {
            indexes.push_back(self.get_index(i)?);
        }
        Ok(indexes)
    }

    pub fn is_binary_content(&self) -> bool {
        self.meta.db_info.content_type == ContentType::Binary
    }

}
