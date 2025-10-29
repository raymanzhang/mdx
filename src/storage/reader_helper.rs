//! Helper functions for reading and decoding dictionary data.
//!
//! This module provides utility functions for:
//! - Character encoding detection and conversion
//! - String encoding/decoding from various encodings (UTF-8, UTF-16LE, etc.)
//! - C-string parsing (null-terminated strings)
//! - Multi-byte and wide character handling

use encoding_rs::Encoding;
use log::debug;
use byteorder::{BigEndian, ReadBytesExt};

use crate::storage::meta_unit::ZdbVersion;
use crate::{Result, ZdbError};

/// Gets an encoding object by its label string.
///
/// # Arguments
///
/// * `label` - Encoding label (e.g., "utf-8", "utf-16", "gbk", "big5")
///
/// # Returns
///
/// Returns a reference to the corresponding Encoding object.
///
/// # Errors
///
/// Returns an error if the encoding label is not recognized.
pub fn get_encoding_object_by_label(label: &str) -> Result<&'static Encoding>{
    let encoding = label.to_lowercase();
    let lable =match encoding.as_str() {
        "utf-16" => "utf-16le",
        _ => encoding.as_str(),
    };
    let encoding_obj = Encoding::for_label(lable.as_bytes());
    match encoding_obj {
        Some(encoding_obj) => Ok( encoding_obj),
        None => Err(ZdbError::invalid_parameter(format!("Invalid encoding: {}", encoding))),
    }
}

/// Extracts a C-style null-terminated string from a byte array.
///
/// # Arguments
///
/// * `cstr` - The byte array containing the C-string
/// * `is_wchar` - Whether the string is wide-character (UTF-16LE)
///
/// # Returns
///
/// Returns a byte slice without the null terminator.
pub fn bytes_from_cstr(cstr:&[u8], is_wchar: bool) -> &[u8]{
    let zero_byte_len: usize = 
        if is_wchar {
            if cstr.len()>2 && cstr[cstr.len()-1]==0 && cstr[cstr.len()-2]==0 { 2 } else {0}
        }else{
            if cstr.len()>1 && cstr[cstr.len()-1]==0 {1} else {0}// Ignore ending zero       
        };
    &cstr[0..cstr.len()-zero_byte_len]
}

fn str_to_utf16le_bytes(s: &str) -> Vec<u8> {
    // Convert &str to UTF-16 encoded u16 vector
    let utf16: Vec<u16> = s.encode_utf16().collect();
    
    // Convert u16 to little-endian byte sequence
    let bytes: Vec<u8> = utf16
        .into_iter()
        .flat_map(|c| c.to_le_bytes()) // Convert to little-endian bytes
        .collect();
    
    bytes
}

/// Encodes a string to bytes using the specified encoding.
///
/// # Arguments
///
/// * `str` - The string to encode
/// * `encoding_obj` - The target encoding
///
/// # Returns
///
/// Returns the encoded bytes.
pub fn encode_string_to_bytes(str: &str, encoding_obj: &'static Encoding) -> Result<Vec<u8>> {
    if encoding_rs::UTF_8 == encoding_obj {
        Ok(str.as_bytes().to_vec())
    }else if encoding_rs::UTF_16LE == encoding_obj {
        Ok(str_to_utf16le_bytes(str))
    }else{
        let (encoded, _, had_errors) = encoding_obj.encode(str);
        if had_errors {
            debug!("Encoding error");
        }
        Ok(encoded.into_owned())
    }
}

/// Decodes bytes to a string using the specified encoding.
///
/// # Arguments
///
/// * `cstr` - The bytes to decode
/// * `encoding_obj` - The source encoding
///
/// # Returns
///
/// Returns the decoded UTF-8 string.
pub fn decode_bytes_to_string(cstr:&[u8], encoding_obj: &'static Encoding) -> Result<String> {
    let cstr=bytes_from_cstr(cstr, encoding_obj.name().to_lowercase().starts_with("utf-16") );
    let (decoded, _, had_errors) = encoding_obj.decode(cstr);
    if had_errors {
        debug!("Decoding error with: {}", encoding_obj.name());
    }
    Ok(decoded.into_owned())
}

pub struct UintReader<R: ReadBytesExt> {
    reader: R,
    version: ZdbVersion,
}

impl<R: ReadBytesExt> UintReader<R> {
    pub fn new(reader: R, version: ZdbVersion) -> Self {
        Self { reader, version }
    }
    #[inline]
    pub fn read_uint(&mut self) -> Result<u64> {
        if self.version == ZdbVersion::V1 {
            Ok(self.reader.read_u32::<BigEndian>()? as u64)
        } else {
            Ok(self.reader.read_u64::<BigEndian>()?)
        }
    }
}