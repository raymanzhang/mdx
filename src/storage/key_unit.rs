use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::rc::Rc;

use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::storage::key_block::KeyBlock;
use crate::storage::key_block_index::KeyBlockIndex;
use crate::storage::key_block_index_unit::KeyBlockIndexUnit;
use crate::storage::meta_unit::MetaUnit;
use crate::storage::unit_base::{read_data_info_section, UnitInfoSection};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename = "KeyData")]
pub struct KeyDataInfo {
    #[serde(rename = "@keyCount")]
    pub key_count: u64,
    #[serde(rename = "@encoding")]
    pub encoding: String,
    #[serde(rename = "@locale", default)]
    pub locale_id: String,
}
//<KeyData keyCount="123" encoding="utf-8" locale="zh-u-co-pinyin" />

pub struct KeyUnit {
    pub total_key_count: u64,
    pub key_data_offset: u64,
    pub block_cache: RefCell<LruCache<u64, Rc<RefCell<KeyBlock>>>>,
    pub meta_info: Rc<MetaUnit>,
}


impl KeyUnit {
    pub fn from_reader_v1_v2<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>, key_block_index_unit: &KeyBlockIndexUnit) -> crate::Result<Self> {
        let key_data_offset = reader.seek(SeekFrom::Current(0))?;
        //Skip to the end of data section
        reader.seek(SeekFrom::Current(key_block_index_unit.key_data_unit_size as i64))?;
        let key_count = key_block_index_unit.total_key_count;
        Ok(Self { 
            total_key_count: key_count, 
            block_cache: RefCell::new(LruCache::new(NonZeroUsize::new(16).unwrap())), 
            meta_info: meta_info.clone(),
            key_data_offset,
        })
    }
    pub fn from_reader_v3<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>) -> crate::Result<Self> {
        let info = UnitInfoSection::from_reader(reader)?;  
        let key_data_offset = reader.seek(SeekFrom::Current(0))?;
        //Skip to the end of data section
        reader.seek(SeekFrom::Current(info.data_section_length as i64))?;
        let mut data_info = read_data_info_section::<KeyDataInfo, R>(reader, &meta_info)?;
        if data_info.locale_id.is_empty() {
            data_info.locale_id = meta_info.db_info.locale_id.clone();
            // if data_info.locale_id.is_empty() {
            //     return Err(ZdbError::invalid_parameter("Empty locale ID"));
            // }
        }
        let key_count = data_info.key_count;
        Ok(Self { 
            total_key_count: key_count, 
            block_cache: RefCell::new(LruCache::new(NonZeroUsize::new(16).unwrap())), 
            meta_info: meta_info.clone(),
            key_data_offset,
        })
    }

    pub fn get_key_block<R: Read+Seek>(
        &self,
        reader: &mut R,
        key_block_index: &KeyBlockIndex,
    ) -> crate::Result<Rc<RefCell<KeyBlock>>> {
        let block_offset = key_block_index.block_offset_in_key_unit;
        if let Some(key_block) = self.block_cache.borrow().peek(&block_offset) {
            return Ok(Rc::clone(key_block));
        }
        reader.seek(SeekFrom::Start(block_offset + self.key_data_offset))?;
        let key_block = Rc::new(RefCell::new(KeyBlock::from_reader(reader, &self.meta_info, key_block_index)?));
        self.block_cache.borrow_mut().put(block_offset, key_block.clone());
        Ok(key_block)
    }
}
        
