//! Sort key generation for dictionary entries.
//!
//! This module provides functions to generate sort keys for different character encodings
//! (UTF-8, UTF-16LE, GBK, Big5). Sort keys are used for fast and accurate string comparison
//! in dictionary lookups.
//!
//! # Examples
//!
//! ```
//! use mdx::sort_key::get_sort_key;
//! use mdx::meta_unit::MetaUnit;
//!
//! // Get sort key for a string
//! let sort_key = get_sort_key("hello", false, false, "utf-8").unwrap();
//! assert!(!sort_key.is_empty());
//! ```

use std::io::Cursor;

use byteorder::{LittleEndian, NativeEndian, ReadBytesExt, WriteBytesExt};

use crate::utils::icu_wrapper::UChar;
use crate::storage::meta_unit::{MetaUnit, ZdbVersion};
use crate::{Result, ZdbError};

/// Checks if two bytes form a valid Big5 character.
pub fn is_big5(c1: u8, c2: u8) -> bool {
    return (c1 >= 0xa1) && (c1 <= 0xf9) 
            && ((c2 >= 0x40) && (c2 <= 0x7e) || (c2 >= 0xa1) && (c2 <= 0xfe));
}

/// Checks if two bytes form a valid GBK character.
pub fn is_gbk(c1: u8, c2: u8) -> bool {
    let ch = c1 as u16 * 256 + c2 as u16;
    return (ch > 0x8140 && ch < 0xfefe) && c2 != 0xff;
}


/// Generates a sort key for a multi-byte encoded string.
///
/// # Arguments
///
/// * `mb_str` - The multi-byte encoded string
/// * `fold_case` - Whether to convert to lowercase
/// * `alpha_and_digit_only` - Whether to keep only alphanumeric characters
/// * `encoding_label` - Character encoding label (e.g., "gbk", "big5", "utf-8")
///
/// # Returns
///
/// Returns the sort key as a byte vector.
pub fn mb_get_sort_key( mb_str: &[u8], fold_case: bool, alpha_and_digit_only: bool, encoding_label: &str) -> Result<Vec<u8>> {
    let mut folded_key = Vec::new();
    let is_gbk_encoding = encoding_label.to_lowercase()=="gbk";
    let is_big5_encoding = encoding_label.to_lowercase()=="big5";

    for i in 0..mb_str.len() {
        let mut ch = mb_str[i];
        if i < mb_str.len() - 1 {
            let nextch = mb_str[i + 1];
            if (is_big5_encoding && is_big5(ch, nextch)) || (is_gbk_encoding &&is_gbk(ch, nextch)) 
            {
                folded_key.push(ch);
                folded_key.push(nextch);
                continue;
            }
        }
        if fold_case {
            if (ch >= b'A') && (ch <= b'Z') {
                ch = ch - b'A' + b'a';
                folded_key.push(ch);
                continue;
            }
        }
        if alpha_and_digit_only {
            if ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch > 127 {
                folded_key.push(ch);
            }
        }else{
            folded_key.push(ch);
        }
    }
    return Ok(folded_key);
}

/// Generates a sort key for a wide character (UTF-16LE) string.
///
/// # Arguments
///
/// * `wc_str` - The wide character string (UTF-16LE encoded)
/// * `fold_case` - Whether to convert to lowercase
/// * `alpha_and_digit_only` - Whether to keep only alphanumeric characters
///
/// # Returns
///
/// Returns the sort key as a byte vector.
pub fn wc_get_sort_key( wc_str: &[u8], fold_case: bool, alpha_and_digit_only: bool) -> Result<Vec<u8>> {
    if wc_str.len()%2!=0 {
        return Err(ZdbError::invalid_data_format("Wide char string length must be even"));
    }
    let mut folded_key = Vec::with_capacity(wc_str.len());
    let mut cursor_in = Cursor::new(wc_str);
    let mut cursor_out = Cursor::new(&mut folded_key);
    for _ in 0..wc_str.len()/2 {
        let wc = cursor_in.read_u16::<LittleEndian>()?;
        if wc<=0xff {
            let mut ch = wc as u8;
            if fold_case {
                if (ch >= b'A') && (ch <= b'Z') {
                    ch = ch - b'A' + b'a';
                    cursor_out.write_u16::<NativeEndian>(ch as u16)?;
                    continue;
                }
            }
            if alpha_and_digit_only {
                if ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch > 127 {
                    cursor_out.write_u16::<NativeEndian>(ch as u16)?;
                }
            }else{
                cursor_out.write_u16::<NativeEndian>(ch as u16)?;
            }
        }else {
            cursor_out.write_u16::<NativeEndian>(wc)?;
        }
    }
    return Ok(folded_key);
}

pub fn get_sort_key(key: &[u8], meta_info: &MetaUnit) -> Result<Vec<u8>> {
    if meta_info.version==ZdbVersion::V3{
        let key_uchar = UChar::try_from(String::from_utf8_lossy(key).into_owned().as_str())?;
        Ok(meta_info.collator.get_sort_key(&key_uchar))
    }else{
        let fold_case = !meta_info.db_info.key_case_sensitive || meta_info.db_info.is_mdd;
        let alpha_and_digit_only = meta_info.db_info.strip_key && !meta_info.db_info.is_mdd;
        if meta_info.db_info.is_utf16 {
                wc_get_sort_key(key, fold_case, alpha_and_digit_only)
        } else {
                mb_get_sort_key(key, fold_case, alpha_and_digit_only, &meta_info.db_info.encoding_label)
        }
    }
}