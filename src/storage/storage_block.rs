//! Storage block handling for compressed and encrypted dictionary data.
//!
//! This module provides functions for reading, decoding, and writing storage blocks
//! within ZDB files. Storage blocks can be compressed and/or encrypted according to
//! the dictionary configuration.

use std::cmp::min;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use crate::utils::compression::{get_compressor, CompressionMethod};
use crate::crypto::digest::ripemd_digest;
use crate::crypto::encryption::{get_encryptor, EncryptionMethod};
use crate::utils::io_utils::read_exact_to_vec;
use crate::storage::meta_unit::MetaUnit;
use crate::ZdbError;

/// A storage block from a ZDB file.
///
/// Storage blocks contain compressed and/or encrypted data that is then decompressed
/// and decrypted as needed when reading dictionary content.
#[derive(Debug, Clone)]
pub struct StorageBlock {
    /// Decompressed and decrypted data
    pub data: Vec<u8>,
}

impl StorageBlock {
    /// Reads and decodes a storage block from a reader (V1/V2 format).
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to read from
    /// * `meta_info` - Dictionary metadata
    /// * `crypto_key` - Encryption key (if applicable)
    /// * `data_block_length` - Compressed block length
    /// * `original_data_length` - Expected uncompressed length
    pub fn from_reader_v1_v2<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit, crypto_key: &[u8], data_block_length: u32, original_data_length: u32) -> crate::Result<Self> {
        let mut raw_data=read_exact_to_vec(reader, data_block_length as usize)?;
        if meta_info.is_v2(){
            let crypto_key = ripemd_digest(&ripemd_digest(&crypto_key)?.as_slice())?;
            return Self::decode_block(&mut raw_data.as_mut_slice(), &crypto_key, original_data_length);
        }else{
            return Self::decode_block(&mut raw_data.as_mut_slice(), &crypto_key, original_data_length);
        }
    }
    
    /// Decodes a storage block (decompresses and decrypts if needed).
    ///
    /// # Arguments
    ///
    /// * `block_data` - The raw block data
    /// * `crypto_key` - Encryption key (if applicable)
    /// * `original_data_length` - Expected uncompressed length
    pub fn decode_block(block_data: &mut [u8], crypto_key: &[u8], original_data_length: u32) -> crate::Result<Self> {
        let mut cursor = Cursor::new(&block_data);
        let compression_encryption = cursor.read_u8()?;
        let encrypted_data_length = cursor.read_u8()?;
        let _reserved = cursor.read_u16::<BigEndian>()?;
        let data_crc = cursor.read_u32::<BigEndian>()?;
        let header_length = cursor.position() as usize;
        drop(cursor);
        //let raw_data_length = block_data.len() - header_length;
        let raw_data = &mut block_data[header_length..];
        
        let encryption_method = EncryptionMethod::try_from((compression_encryption&0xF0)>>4)?;
        if encryption_method != EncryptionMethod::None {
            let crypto_key = if crypto_key.is_empty() {
                ripemd_digest(&data_crc.to_be_bytes())?
            } else {
                crypto_key.to_vec()
            };
            
            let mut decryptor = get_encryptor(encryption_method, &crypto_key, &[0; 8])?;
            let input = &mut raw_data[0..encrypted_data_length as usize];
            let mut output = vec![0u8; input.len() as usize];
            decryptor.decrypt(&input, &mut output)?;
            input.copy_from_slice(&output); //input is part of raw_data, now raw_data is decrypted        
        }

        let crc_is_for_compressed_data=encryption_method != EncryptionMethod::None;
        if crc_is_for_compressed_data  {
            //Crc is for compressed data, not for encrypted data
            let alder_crc = adler::adler32_slice(&raw_data);
            if data_crc != alder_crc {
                return Err(ZdbError::crc_mismatch(data_crc, alder_crc));
            }
        }

        let compression_method = CompressionMethod::try_from(compression_encryption&0x0F)?;
        let decompressor =  get_compressor(compression_method);
        let data= decompressor.decompress(&raw_data, original_data_length as usize)?;
        if !crc_is_for_compressed_data {
            let alder_crc = adler::adler32_slice(&data);
            if data_crc != alder_crc {
                return Err(ZdbError::crc_mismatch(data_crc, alder_crc));
            }
        }
//        Ok(Self { original_data_length, compressed_data_length, next_data_section_length, compression_encryption, encrypted_data_length, reserved, crc: compressed_data_crc, data })
        Ok(Self { data })
    }

    pub fn from_reader_v3<R: Read+Seek>(reader: &mut R, meta_info: &MetaUnit) -> crate::Result<Self> {
        let original_data_length = reader.read_u32::<BigEndian>()?; //original_data_length is the length of uncompressed data length
        let data_block_length = reader.read_u32::<BigEndian>()?; //Data block length is raw compressed data length + header length
        let mut raw_data=read_exact_to_vec(reader, data_block_length as usize)?;
        return Self::decode_block(&mut raw_data, &meta_info.crypto_key, original_data_length);
    }

    pub fn to_writer<W: Write+Seek>(writer: &mut W, data:&[u8], crypto_key:&[u8], compression_method:CompressionMethod, encryption_method:EncryptionMethod) -> crate::Result<u64> {
        let pos = writer.seek(SeekFrom::Current(0))?;
        let compressor =  get_compressor(compression_method);
        let mut encryptor = get_encryptor(encryption_method, &crypto_key, &[0; 8])?;

        let mut compression_encryption = (compression_method as u8) | (encryption_method as u8)<<4;
        let mut compressed_data = compressor.compress(data)?;
        let mut encrypted_data_length=min(32,compressed_data.len());
        
        // Determine if we will actually encrypt data
        let will_encrypt = data.len() >= encrypted_data_length && encryption_method != EncryptionMethod::None;
        
        // Calculate CRC based on whether encryption will be applied
        // If encryption is applied, CRC is for compressed data (matching reader's logic at line 54)
        // If no encryption, CRC is for original uncompressed data (matching reader's logic at line 67)
        let data_crc = if will_encrypt {
            adler::adler32_slice(&compressed_data)
        } else {
            adler::adler32_slice(data)
        };
        
        if will_encrypt {
            let mut encrypted_data= vec![0u8; encrypted_data_length];
            encryptor.encrypt(&compressed_data[0..encrypted_data_length], &mut encrypted_data)?;
            compressed_data[0..encrypted_data.len()].copy_from_slice(&encrypted_data); //Replace with encrypted data
        } else {
            encrypted_data_length=0;
            compression_encryption=compression_method as u8;
        }
        const HEADER_LENGTH:u32=8;
        writer.write_u32::<BigEndian>(data.len() as u32)?; //original_data_length
        writer.write_u32::<BigEndian>(compressed_data.len() as u32+HEADER_LENGTH)?; //compressed_data_length
        writer.write_u8(compression_encryption)?;
        writer.write_u8(encrypted_data_length as u8)?;
        writer.write_u16::<BigEndian>(0)?; //reserved
        writer.write_u32::<BigEndian>(data_crc)?; //data_crc
        writer.write_all(&compressed_data)?;
        Ok(writer.seek(SeekFrom::Current(0))? - pos)
    }
}
