use std::io::{Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};

use crate::utils::io_utils::{read_exact_to_vec};
use crate::storage::UintReader;
use crate::storage::meta_unit::{MetaUnit, ZdbVersion};
use crate::storage::storage_block::StorageBlock;
use crate::storage::unit_base::{read_data_info_section, UnitInfoSection};
use crate::Result;

#[derive(Debug, Clone, Default)]
pub struct ContentBlockIndex {
    pub block_original_length: u64,
    pub block_compressed_length: u64,
    pub block_offset_in_source: u64,
    pub block_offset_in_unit: u64,
} 

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename = "RecordIndex")]
pub struct ContentBlockIndexDataInfo {
    #[serde(rename = "@encoding")]
    pub encoding: String,
    #[serde(rename = "@recordCount")]
    pub record_count: u64,
}
//  <RecordIndex encoding="binary" recordCount="10" />


pub struct ContentBlockIndexUnit {
        // pub info: UnitInfoSection,
        // pub data_info: ContentBlockIndexDataInfo,
    pub record_count: u64,
    pub block_index_entries: Vec<ContentBlockIndex>,
    pub total_original_data_length: u64,
} 

impl ContentBlockIndex {

    fn from_reader<R:ReadBytesExt>(reader: &mut R, version: &ZdbVersion) -> Result<Self> {
        let block_compressed_length = match version {
            ZdbVersion::V3 | ZdbVersion::V2=> reader.read_u64::<BigEndian>()?,
            ZdbVersion::V1 => reader.read_u32::<BigEndian>()? as u64,
        };
        let block_original_length = match version {
            ZdbVersion::V3 | ZdbVersion::V2=> reader.read_u64::<BigEndian>()?,
            ZdbVersion::V1 => reader.read_u32::<BigEndian>()? as u64,
        };
        Ok(Self { block_original_length, block_compressed_length, block_offset_in_source: 0, block_offset_in_unit: 0 })
    }
}

impl ContentBlockIndexUnit {
    fn read_block_index_entries(block_data: &Vec<u8>, meta_info: &MetaUnit, block_count: u32) -> Result<(Vec<ContentBlockIndex>, u64)> {
        let mut block_index_entries:Vec<ContentBlockIndex> = Vec::with_capacity(block_count as usize);
        let mut block_data_reader = Cursor::new(block_data);
        for _ in 0..block_count {
                let entry = ContentBlockIndex::from_reader(&mut block_data_reader, &meta_info.version)?;
                block_index_entries.push(entry);
        }
    
        let mut block_offset_in_unit = 0;
        let mut block_offset_in_source = 0;
        for block_index in block_index_entries.iter_mut() {
            block_index.block_offset_in_source = block_offset_in_source;
            block_offset_in_source += block_index.block_original_length;
            block_index.block_offset_in_unit = block_offset_in_unit;
            block_offset_in_unit += block_index.block_compressed_length;
        }
        Ok((block_index_entries, block_offset_in_source))
    }
    
    pub fn from_reader_v3<R: Read + Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>, block_index_count: u32) -> crate::Result<Self> {
        let info = UnitInfoSection::from_reader(reader)?;

        //Need to read data_info first for encoding information.
        let cur_pos=reader.seek( SeekFrom::Current(0))?;
        reader.seek(SeekFrom::Current(info.data_section_length as i64))?; //skip to the end of data section
        let data_info = read_data_info_section::<ContentBlockIndexDataInfo, R>(reader, &meta_info)?;
        let end_of_unit = reader.seek(SeekFrom::Current(0))?;
        //Rollback to the beginning of data section
        reader.seek(SeekFrom::Start(cur_pos))?;
        let block_index_data = StorageBlock::from_reader_v3(reader, &meta_info)?.data;
        let (block_index_entries, total_original_data_length) = Self::read_block_index_entries(&block_index_data, &meta_info, block_index_count as u32)?;    
        reader.seek(SeekFrom::Start(end_of_unit))?;
        let record_count = data_info.record_count;
        Ok(Self { record_count, block_index_entries, total_original_data_length})
    }

    pub fn from_reader_v1_v2<R: Read + Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>) -> crate::Result<Self> {
        let idx_para = read_exact_to_vec(reader, if meta_info.is_v1() {4*4} else {8*4})?;
        let mut idx_para_reader = UintReader::new(Cursor::new(&idx_para), meta_info.version);

        let block_count = idx_para_reader.read_uint()?;
        let record_count = idx_para_reader.read_uint()?;
        let content_block_index_size = idx_para_reader.read_uint()?; //No compression in V1 and V2
        let _content_data_block_comp_size = idx_para_reader.read_uint()?; 
    
        drop(idx_para_reader);
        let block_index_data = read_exact_to_vec(reader, content_block_index_size as usize)?;
        let (block_index_entries, total_original_data_length) = Self::read_block_index_entries(&block_index_data, &meta_info, block_count as u32)?;
        Ok(Self { record_count, block_index_entries, total_original_data_length })
    }
}

impl ContentBlockIndexUnit {
    pub fn get_index(&self, offset: u64) -> Result<ContentBlockIndex> {
        let entries = &self.block_index_entries;
        let mut left = 0;
        let mut right = entries.len();
        while left < right {
            let mid = left + (right - left) / 2;
            let entry = &entries[mid];
            let start = entry.block_offset_in_source;
            let end = start + entry.block_original_length;
            if offset < start {
                if mid == 0 { break; }
                right = mid;
            } else if offset >= end {
                left = mid + 1;
            } else {
                return Ok(entry.clone());
            }
        }
        Err(crate::ZdbError::invalid_parameter("offset not found in any block index entry"))
    }
}