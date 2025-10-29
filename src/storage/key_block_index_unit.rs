use std::io::{Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};

use crate::crypto::digest::ripemd_digest;
use crate::crypto::encryption::{Encryptor, Salsa20Encryptor, SimpleEncryptor};
use crate::utils::io_utils::{read_exact_to_vec};
use crate::storage::UintReader;
use super::key_block::EntryNo;
use super::key_block_index::KeyBlockIndex;
use crate::storage::meta_unit::MetaUnit;
use crate::storage::storage_block::StorageBlock;
use crate::storage::unit_base::{read_data_info_section, UnitInfoSection};
use crate::utils::{binary_search_first, RandomAccessable};
use crate::{Result, ZdbError};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename = "KeyBlockIndex")]
pub struct KeyBlockIndexDataInfo {
    #[serde(rename = "@blockCount")]
    pub block_count: u32,
    #[serde(rename = "@encoding")]
    pub encoding: String,
    #[serde(rename = "@locale", default)]
    pub locale_id: String,
}
// <KeyBlockIndex BlockCount="5" encoding="utf-8" locale="zh-u-co-pinyin" />

pub struct KeyBlockIndexUnit {
    pub block_indexes: Vec<KeyBlockIndex>,
    pub meta_info: Rc<MetaUnit>,
    pub total_key_count: u64,
    pub key_data_unit_size: u64, //Only used in V1 and V2
}

impl RandomAccessable<KeyBlockIndex> for KeyBlockIndexUnit{
    fn get_item(&self, index:usize)->Result<&KeyBlockIndex> {
        return Ok(&self.block_indexes[index as usize]);
    }
    fn len(&self)->usize {
        return self.block_indexes.len();
    }
}

impl KeyBlockIndexUnit {
    pub fn find_index(&self, key: &str, prefix_match: bool, partial_match: bool) -> Result<Option<KeyBlockIndex>> {
        let meta_info = self.meta_info.clone();
        binary_search_first(self, key, &meta_info, prefix_match, partial_match)
    }
    pub fn get_index(&self, entry_no: EntryNo) -> Result<&KeyBlockIndex> {
        let mut left = 0;
        let mut right = self.block_indexes.len();
        while left < right {
            let mid = (left + right) / 2;
            let block = &self.block_indexes[mid];
            let start = block.first_entry_no_in_block;
            let end = start + block.entry_count_in_block as EntryNo - 1;
            if entry_no < start {
                right = mid;
            } else if entry_no > end {
                left = mid + 1;
            } else {
                return Ok(block);
            }
        }
        Err(ZdbError::invalid_parameter("Index out of range"))
    }
}

impl KeyBlockIndexUnit {
    /// 公共解码和编号分配逻辑

    fn read_idx_para_v1_v2<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit) -> Result<Vec<u8>> {
        let mut idx_para = if meta_info.is_v2(){vec![0; 8*5]}else{vec![0; 4*4]};
        reader.read_exact(&mut idx_para)?;
        if meta_info.is_v2(){

            if !meta_info.crypto_key.is_empty() && meta_info.db_info.encryption_type.is_para_encrypted() {
                let mut decryptor = Salsa20Encryptor::new(meta_info.crypto_key.as_slice(), &vec![0;8]);
                let mut decrypted_idx_para = vec![0; idx_para.len()];
                decryptor.decrypt(&idx_para, &mut decrypted_idx_para)?;
                idx_para = decrypted_idx_para;
            }
            let crc = reader.read_u32::<BigEndian>()?;
            let checksum = adler::adler32_slice(&idx_para);
            if crc != checksum {
                return Err(ZdbError::crc_mismatch(crc, checksum));
            }
        }
        Ok(idx_para)
    }

    fn read_block_index_data<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit, block_data_size:u64, original_data_length:u64) -> Result<Vec<u8>> {
        let mut raw_data = read_exact_to_vec(reader, block_data_size as usize)?;        

        let block_index_data = if meta_info.is_v2() {
            if meta_info.db_info.encryption_type.is_data_encrypted() {
                let mut enc_key = [0;8];
                enc_key[0..4].copy_from_slice(&raw_data[4..8]);
                enc_key[4..8].copy_from_slice(&0x3695u32.to_le_bytes());
                let mut decryptor = SimpleEncryptor::new(&ripemd_digest(&enc_key)?, &[0;8]);
                decryptor.inplace_decrypt(&mut raw_data[8..])?;
            }
            StorageBlock::decode_block(&mut raw_data, &meta_info.crypto_key, original_data_length as u32)?.data
        } else {
            raw_data
        };
        Ok(block_index_data)
    }

    fn read_block_index_entries(block_data: &Vec<u8>, meta_info: &MetaUnit, block_count: u32) -> Result<(Vec<KeyBlockIndex>, u64)> {
        let mut cursor = Cursor::new(block_data); 
        let mut block_index_entries:Vec<KeyBlockIndex> = Vec::with_capacity(block_count as usize);
        for _ in 0..block_count {
            let entry = KeyBlockIndex::from_reader(&mut cursor, &meta_info)?;
            block_index_entries.push(entry);
        }
    
        let mut block_offset_in_unit = 0;
        let mut first_entry_no_in_block = 0;
        for block_index in block_index_entries.iter_mut() {
            block_index.first_entry_no_in_block = first_entry_no_in_block;
            first_entry_no_in_block += block_index.entry_count_in_block as EntryNo;
            block_index.block_offset_in_key_unit = block_offset_in_unit;
            block_offset_in_unit += block_index.block_length;
        }
        Ok((block_index_entries, first_entry_no_in_block as u64))
    }

    pub fn from_reader_v1_v2<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>) -> Result<Self> {
        let idx_para = Self::read_idx_para_v1_v2(reader, &meta_info)?;
        let mut idx_para_reader = UintReader::new(Cursor::new(&idx_para), meta_info.version);
        let key_block_count = idx_para_reader.read_uint()?;
        let record_count = idx_para_reader.read_uint()?;
        let key_index_section_orig_size = idx_para_reader.read_uint()?;
        let key_index_section_comp_size = if meta_info.is_v1() {key_index_section_orig_size} else {idx_para_reader.read_uint()?};
        let key_data_section_comp_size = idx_para_reader.read_uint()?;
        drop(idx_para_reader);

        let block_index_data = Self::read_block_index_data(reader, &meta_info, key_index_section_comp_size, key_index_section_orig_size)?;
        let (block_index_entries, total_key_count) = Self::read_block_index_entries(&block_index_data, &meta_info, key_block_count as u32)?;

        if total_key_count != record_count {
            return Err(ZdbError::invalid_data_format(format!("Total key count {} does not match record count {}", total_key_count, record_count)));
        }
        Ok(Self { block_indexes: block_index_entries, meta_info: meta_info.clone(), total_key_count, key_data_unit_size: key_data_section_comp_size })
    }
    
    pub fn from_reader_v3<R: Read+Seek>(reader: &mut R, meta_info: &Rc<MetaUnit>) -> crate::Result<Self> {
        let unit_info = UnitInfoSection::from_reader(reader)?;
        //Need to read data_info first for encoding information.
        let cur_pos=reader.seek( SeekFrom::Current(0))?;
        reader.seek(SeekFrom::Current(unit_info.data_section_length as i64))?; //skip to the end of data section
        let mut data_info = read_data_info_section::<KeyBlockIndexDataInfo, R>(reader, &meta_info)?;
        let end_of_unit = reader.seek(SeekFrom::Current(0))?;
        if data_info.locale_id.is_empty() {
            data_info.locale_id = meta_info.db_info.locale_id.clone();
            // if data_info.locale_id.is_empty() {
            //     return Err(ZdbError::invalid_parameter("Empty locale ID"));
            // }
        }
        //Rollback to the beginning of data section
        reader.seek(SeekFrom::Start(cur_pos))?;
        let storage_block = StorageBlock::from_reader_v3(reader, &meta_info)?;
        let (block_index_entries, total_key_count) = Self::read_block_index_entries(&storage_block.data, &meta_info, data_info.block_count)?;
        reader.seek(SeekFrom::Start(end_of_unit))?;
        Ok(Self { 
            block_indexes: block_index_entries, 
            meta_info: meta_info.clone(),
            total_key_count,
            key_data_unit_size: 0, //Not used in V3
        })
    }
}
