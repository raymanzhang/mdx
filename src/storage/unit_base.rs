use std::io::{Read, Seek, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::utils::compression::CompressionMethod;
use crate::crypto::encryption::EncryptionMethod;
use crate::storage::meta_unit::MetaUnit;
use crate::storage::reader_helper::bytes_from_cstr;
use crate::storage::storage_block::StorageBlock;
use crate::utils::remove_xml_declaration;
use crate::{Result, ZdbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum UnitType {
    #[default]
    Invalid = 0,
    Content = 1,
    ContentBlockIndex = 2,
    Key = 3,
    KeyBlockIndex = 4,
}

impl TryFrom<u8> for UnitType {
    type Error = ZdbError;
    
    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(UnitType::Content),
            2 => Ok(UnitType::ContentBlockIndex),
            3 => Ok(UnitType::Key),
            4 => Ok(UnitType::KeyBlockIndex),
            _ => Err(ZdbError::invalid_parameter(format!("Invalid unit type:{}",value))),
        }
    }
}


#[derive(Debug, Clone, Default)]
pub struct UnitInfoSection {
    pub unit_type: UnitType, //1: content, 2: content block index, 3: key, 4: key block index
    pub _reserved1: [u8; 3],
    pub _reserved2: u64, //Total unit length - 12, redundant data
    pub block_count: u32, //block count in unit
    pub data_section_length: u64, //data section length in bytes
    pub orig_data_section_length: u64, //Only available in V1-V2
}

// pub trait UnitDeserialize: Sized {
//     fn from_reader<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit) -> Result<Self>;
// }


impl UnitInfoSection {
    pub fn from_reader<R: Read>(reader: &mut R) -> crate::Result<Self> {
        let unit_type = UnitType::try_from(reader.read_u8()?)?;
        let mut reserved1 = [0u8; 3];
        reader.read_exact(&mut reserved1)?;
        let reserved2 = reader.read_u64::<BigEndian>()?;
        let block_count = reader.read_u32::<BigEndian>()?;
        let data_section_length = reader.read_u64::<BigEndian>()?;
        Ok(Self { unit_type, _reserved1: reserved1, _reserved2: reserved2, block_count, data_section_length, orig_data_section_length:0 })
    }
    pub fn to_writer<W: Write+Seek>(&self, writer: &mut W) -> crate::Result<()> {
        writer.write_u8(self.unit_type as u8)?;
        writer.write_all(&self._reserved1)?;
        writer.write_u64::<BigEndian>(self._reserved2)?;
        writer.write_u32::<BigEndian>(self.block_count)?;
        writer.write_u64::<BigEndian>(self.data_section_length)?;
        Ok(())
    }
}

pub fn read_data_info_section<T, R>(reader: &mut R, meta_info: &MetaUnit) -> Result<T>
where
    T: DeserializeOwned,
    R: Read+Seek,
{
    let block_data = StorageBlock::from_reader_v3(reader, meta_info)?;
    let raw_xml = String::from_utf8(bytes_from_cstr(&block_data.data, false).to_vec())?;
    let data_info: T = serde_xml_rs::from_str(&raw_xml)?;
    Ok(data_info)
}

pub fn write_data_info_section<T, W>(writer: &mut W, data_info: &T, crypto_key:&[u8], compression_method:CompressionMethod, encryption_method:EncryptionMethod) -> crate::Result<()>
where
    T: Serialize,
    W: Write+Seek,
{
    let mut raw_xml = serde_xml_rs::to_string(data_info)?;
    remove_xml_declaration(&mut raw_xml);
    StorageBlock::to_writer(writer, &raw_xml.as_bytes(), crypto_key, compression_method, encryption_method)?;
    Ok(())
}