use std::cmp::Ordering;
use std::io::Read;

use byteorder::{BigEndian, ReadBytesExt};

use crate::utils::io_utils::read_exact_to_vec;
use super::key_block::EntryNo;
use crate::storage::meta_unit::{MetaUnit, ZdbVersion};
use crate::storage::reader_helper::decode_bytes_to_string;
use crate::utils::sort_key::get_sort_key;
use crate::utils::{key_compare, KeyComparable};
use crate::Result;

#[derive(Debug, Clone, Default)]
pub struct KeyBlockIndex {
    pub entry_count_in_block: u64,
    pub first_key: String,
    pub last_key: String,
    pub first_sort_key: Vec<u8>,
    pub last_sort_key: Vec<u8>,
    pub block_length: u64,
    pub raw_data_length: u64,
    pub block_offset_in_key_unit: u64,
    pub first_entry_no_in_block: EntryNo,
}

impl KeyComparable for KeyBlockIndex {
    fn compare_with(&self, other:&str, other_sort_key:&[u8], prefix_match: bool, meta_info: &MetaUnit) -> Result<Ordering> {
        match key_compare(&self.first_key, &self.first_sort_key, other, other_sort_key, prefix_match, meta_info) {
           Ok(Ordering::Less)  => match key_compare(&self.last_key, &self.last_sort_key, other, other_sort_key, prefix_match, meta_info) {
                Ok(Ordering::Greater) | Ok(Ordering::Equal) => Ok(Ordering::Equal),
                Ok(Ordering::Less) => Ok(Ordering::Less),
                Err(e) => Err(e),
            },
           Ok(Ordering::Greater) => Ok(Ordering::Greater),
           Ok(Ordering::Equal) => Ok(Ordering::Equal),
           Err(e) => Err(e),
        } 
    }
}

fn read_key<R:Read>(reader: &mut R, meta_info: &MetaUnit)->Result<Vec<u8>> {
    let mut length = match meta_info.version {
        ZdbVersion::V3 | ZdbVersion::V2 => reader.read_u16::<BigEndian>()? as usize,
        ZdbVersion::V1 => reader.read_u8()? as usize,
    };
    //V1's key dosen't has a terminating zero
    let has_terminating_zero = if meta_info.is_v1() {0} else {if meta_info.db_info.is_utf16 {2} else {1}}; 

    //UTF-16 key length is in char count, we need to convert it to byte count
    if meta_info.db_info.is_utf16{
        length = length*2;
    }
    //Key length dosen't include the terminating zero for all versions
    //We need to read the terminating zero for V2 and V3.
    let mut buffer = read_exact_to_vec(reader, length+has_terminating_zero)?;
    // if length<has_terminating_zero {
    //     return Err(ZdbError::InvalidDataFormat(format!("Key length {} is less than terminating zero size {}", length, has_terminating_zero)));
    // }
    buffer.truncate(length);
    Ok(buffer)
}

impl KeyBlockIndex {
    pub fn from_reader<R:Read>(reader: &mut R, meta_info: &MetaUnit) -> Result<Self> {
        let (entry_count, first_key, last_key, block_length, raw_data_length) = match meta_info.version {
            ZdbVersion::V3|ZdbVersion::V1 => {
                (
                    reader.read_u32::<BigEndian>()? as u64,
                    read_key(reader, meta_info)?,
                    read_key(reader, meta_info)?,
                    reader.read_u32::<BigEndian>()? as u64,
                    reader.read_u32::<BigEndian>()? as u64
                )
           },
            ZdbVersion::V2 => {
                (
                    reader.read_u64::<BigEndian>()?,
                    read_key(reader, meta_info)?,
                    read_key(reader, meta_info)?,
                    reader.read_u64::<BigEndian>()?,
                    reader.read_u64::<BigEndian>()?
                )
            }
        };
        let  first_sort_key=get_sort_key(&first_key, meta_info)?;
        let  last_sort_key=get_sort_key(&last_key, meta_info)?;
        let first_key = decode_bytes_to_string(&first_key, &meta_info.encoding_obj)?;
        let last_key = decode_bytes_to_string(&last_key, &meta_info.encoding_obj)?;

        Ok(Self {
            entry_count_in_block: entry_count,
            first_key,
            last_key,
            first_sort_key,
            last_sort_key,
            block_length,
            raw_data_length,
            block_offset_in_key_unit: 0,
            first_entry_no_in_block: 0,
        })
    }
}
