//! Dictionary metadata and version information.
//!
//! This module defines the metadata structures used in MDX/MDD files, including:
//! - ZDB file format versions (V1, V2, V3)
//! - Content types (Text, HTML, Binary)
//! - Database information and configuration
//! - Encryption and compression settings
//!
//! # Examples
//!
//! ```no_run
//! use mdx::meta_unit::{MetaUnit, ZdbVersion, ContentType};
//!
//! // Version information
//! let version = ZdbVersion::from_version_number(300)?;
//! assert_eq!(version, ZdbVersion::V3);
//!
//! // Content type detection
//! let content_type = ContentType::from_str("html")?;
//! assert_eq!(content_type, ContentType::Html);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::io::{Read, Seek};
use std::rc::Rc;

use byteorder::{BigEndian, ReadBytesExt};
use encoding_rs::Encoding;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

use crate::crypto::digest::{fast_hash_digest, ripemd_digest};
use crate::crypto::encryption::decrypt_salsa20;
use crate::utils::icu_wrapper::UCollator;
use crate::storage::reader_helper::{decode_bytes_to_string, get_encoding_object_by_label};
use crate::{Result, ZdbError};

/// ZDB file format version.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum ZdbVersion {
    /// Version 1 format (legacy)
    V1 = 1,
    /// Version 2 format
    V2 = 2,
    /// Version 3 format (current)
    #[default]
    V3 = 3,
}

impl ZdbVersion {
    /// Converts a version number to a ZdbVersion.
    ///
    /// # Arguments
    ///
    /// * `version` - Version number (e.g., 100, 200, 300)
    ///
    /// # Returns
    ///
    /// Returns the corresponding ZdbVersion.
    ///
    /// # Errors
    ///
    /// Returns an error if the version is not supported.
    pub fn from_version_number(version: u32) -> Result<Self> {
        // #[cfg(feature = "rust-icu")]
        // if version >300 {
        //     return Err(ZdbError::invalid_data_format("Unsupported engine version, rust-icu is not supported for version >300"));
        // }

        // #[cfg(not(feature = "icu"))]
        // if version == 300 {
        //     return Err(ZdbError::invalid_data_format("Unsupported engine version, icu is not supported for version =300"));
        // }
        let version = match version / 100 {
            1 => ZdbVersion::V1,
            2 => ZdbVersion::V2,
            3 => ZdbVersion::V3,
            _ => return Err(ZdbError::invalid_data_format(format!("Unsupported engine version: {}", version))),
        };
        Ok(version)
    }
}

/// Content type stored in the dictionary.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ContentType {
    /// Plain text content
    #[default]
    Text,
    /// HTML formatted content
    Html,
    /// Binary data
    Binary,
}

impl ContentType {
    /// Converts a string to a ContentType.
    ///
    /// # Arguments
    ///
    /// * `s` - Content type string ("text", "html", or "binary")
    ///
    /// # Returns
    ///
    /// Returns the corresponding ContentType.
    ///
    /// # Errors
    ///
    /// Returns an error if the content type is not recognized.
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "text" => Ok(ContentType::Text),
            "html" => Ok(ContentType::Html),
            "binary" => Ok(ContentType::Binary),
            _ => Err(ZdbError::invalid_data_format(format!("Unsupported content type:{}",s))),
        }
    }
}

/// Key block index encryption type.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize, Debug)]
pub enum KeyBlockIndexEncrytionType {
    /// No encryption
    #[default]
    None = 0,
    /// Index and paragraph encrypted
    IndexPara = 1,
    /// Index and data encrypted
    IndexData = 2,
    /// Paragraph and data encrypted
    ParaAndData = 3,
}

impl TryFrom<u32> for KeyBlockIndexEncrytionType {
    type Error = ZdbError;

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(KeyBlockIndexEncrytionType::IndexPara),
            2 => Ok(KeyBlockIndexEncrytionType::IndexData),
            3 => Ok(KeyBlockIndexEncrytionType::ParaAndData),
            _ => Err(ZdbError::invalid_data_format("Invalid value for EncryptionType")),
        }
    }
}

impl KeyBlockIndexEncrytionType {
    /// Checks if this encryption type encrypts data.
    pub fn is_encrypted(&self) -> bool {
        self!=&KeyBlockIndexEncrytionType::None
    }
    /// Checks if this encryption type encrypts paragraph data.
    pub fn is_para_encrypted(&self) -> bool {
        self == &KeyBlockIndexEncrytionType::IndexPara || self == &KeyBlockIndexEncrytionType::ParaAndData
    }
    /// Checks if this encryption type encrypts index data.
    pub fn is_data_encrypted(&self) -> bool {
        self == &KeyBlockIndexEncrytionType::IndexData || self == &KeyBlockIndexEncrytionType::ParaAndData
    }
}

// ZDB XML header format:
//   <ZDB GeneratedByEngineVersion="3.0" 
//       RequiredEngineVersion="3.0" 
//       ContentType="Html" 
//       RegisterBy="EMail" 
    // RequiredEngineVersion="3.0" 
    // ContentType="Html" 
    // RegisterBy="EMail" 
    // Description="" 
    // Title="" 
    // DefaultSortingLocale="zh-u-co-pinyin-ks-level1-ka-shifted-kr-space-punct-symbol-digit-en-others-hani" 
    // UUID="b7c4f2a0-9930-4ebe-8ef2-be47a587bdea" 
    // CreationDate="2021-2-26" 
    // Compact="No" 
    // DataSourceFormat="107" 
    // StyleSheet=""/>

//<ZDB GeneratedByEngineVersion="3.0" 
    //RequiredEngineVersion="3.0" 
    //ContentType="Binary" 
    //RegisterBy="EMail" 
    //Description="" 
    //Title="" 
    //DefaultSortingLocale="" 
    //UUID="be335fe3-139b-4b28-8d48-a264d8fe7585" CreationDate="2024-4-20"/>

//<Dictionary GeneratedByEngineVersion="2.0" 
    // RequiredEngineVersion="2.0" 
    // Format="Html" 
    // KeyCaseSensitive="No" 
    // StripKey="No" 
    // Encrypted="0" 
    // RegisterBy="EMail" 
    // Description="Merriam-Webster Dictionary Online" 
    // Title="" 
    // Encoding="UTF-8" 
    // CreationDate="2017-8-13" 
    // Compact="Yes" 
    // Compat="Yes" 
    // Left2Right="Yes" 
    // DataSourceFormat="106" 
    // StyleSheet=""/>
#[derive(Clone, Default, Debug)]
pub struct DbInfo {
    pub tag:String,
    
    //For all version
    pub version: ZdbVersion, 
    pub description:String,
    pub title:String,
    pub is_compact_format:bool,
    pub register_by:String,
    pub creation_date:String,
    pub data_source_format:u32,
    pub style_sheet:String,

    //For version 3.0
    pub uuid: String,
    pub locale_id: String,
    pub content_type: ContentType,
    
    //For version <3.0
    pub encryption_type: KeyBlockIndexEncrytionType, //Only used in version <300
    pub key_case_sensitive:bool,
    pub strip_key:bool,
    pub embedded_reg_code: String,
    pub lib_sn:String,
    pub encoding_label:String,
    pub _left_to_right:bool, 

    pub is_mdd:bool,
    pub is_utf16:bool,
}

fn get_node_attr_str(attrs: &[(String, String)], key: &str) -> String {
    for (attr_key, attr_value) in attrs {
        if attr_key == key {
            return attr_value.clone();
        }
    }
    String::new()
}

fn get_node_attr_bool(attrs: &[(String, String)], key: &str, default: bool) -> bool {
    let value = get_node_attr_str(attrs, key).to_lowercase();
    if value == "yes" {
        return true;
    } else if value == "no" {
        return false;
    }
    default
}

fn get_node_attr_u32(attrs: &[(String, String)], key: &str) -> u32 {
    get_node_attr_str(attrs, key).parse::<u32>().unwrap_or_default()
}

fn generate_locale_id(encoding_label: &str, key_case_sensitive:bool, strip_key:bool) -> String {
    let mut locale_id = String::new();
    match encoding_label.to_lowercase().as_str(){
        "gbk" => locale_id.push_str("zh-Hans-u-co-pinyin"),
        "big5" => locale_id.push_str("zh-Hant-u-co-pinyin"),
        _ => locale_id.push_str("en-u"),
    }
    if !key_case_sensitive {
        locale_id.push_str("-ks-level2");
    }else{
        locale_id.push_str("-ks-level3");
    }
    if strip_key {
        locale_id.push_str("-ka-shifted");
    }

    locale_id
}
impl DbInfo {
    pub fn from_xml(xml: &str) -> Result<Self> {
        let mut db_info = DbInfo::default();
        let mut reader = quick_xml::Reader::from_str(xml);
        
        let mut buf = Vec::new();
        let mut root_attrs = Vec::new();
        let mut root_name = String::new();
        
        loop {
            let event = reader.read_event_into(&mut buf)
                .map_err(|e| ZdbError::invalid_data_format(format!("Failed to parse XML: {}", e)))?;
            
            match event {
                Event::Start(e) | Event::Empty(e) => {
                    if root_name.is_empty() {
                        root_name = std::str::from_utf8(e.name().as_ref())
                            .map_err(|e| ZdbError::invalid_data_format(format!("Invalid UTF-8 in XML: {}", e)))?
                            .to_string();
                        
                        // Collect all attributes into a Vec<(String, String)>
                        for attr_result in e.attributes() {
                            let attr = attr_result
                                .map_err(|e| ZdbError::invalid_data_format(format!("Failed to parse XML attributes: {}", e)))?;
                            let key = std::str::from_utf8(attr.key.as_ref())
                                .map_err(|e| ZdbError::invalid_data_format(format!("Invalid UTF-8 in attribute key: {}", e)))?
                                .to_string();
                            let value = std::str::from_utf8(attr.value.as_ref())
                                .map_err(|e| ZdbError::invalid_data_format(format!("Invalid UTF-8 in attribute value: {}", e)))?
                                .to_string();
                            root_attrs.push((key, value));
                        }
                        break;
                    }
                }
                Event::Eof => {
                    return Err(ZdbError::invalid_data_format("No root element found in XML"));
                }
                _ => continue,
            }
        }
        
        db_info.tag = root_name.to_lowercase();
        db_info.is_mdd = db_info.tag == "library_data"; //If the tag is library_data, it's a mdd file
        db_info.version = ZdbVersion::from_version_number((get_node_attr_str(&root_attrs, "RequiredEngineVersion").parse::<f32>().unwrap_or_default()*100.0) as u32)?;
        db_info.encryption_type = get_node_attr_u32(&root_attrs, "Encrypted").try_into().unwrap_or_default();
        db_info.uuid = get_node_attr_str(&root_attrs,"UUID");

        let mut content_type= if db_info.version != ZdbVersion::V3 {
            get_node_attr_str(&root_attrs,"Format")
        } else {
            get_node_attr_str(&root_attrs,"ContentType")
        };
        if content_type.is_empty() {
            content_type = "binary".to_string();
        }
        db_info.content_type = ContentType::from_str(&content_type)?;
        db_info.is_mdd = if let ContentType::Binary = db_info.content_type { true } else { false };        

        db_info.locale_id = get_node_attr_str(&root_attrs,"DefaultSortingLocale");
        db_info.embedded_reg_code = get_node_attr_str(&root_attrs,"RegCode");
        db_info.lib_sn = get_node_attr_str(&root_attrs,"LibSN");
        db_info.encoding_label = get_node_attr_str(&root_attrs,"Encoding").to_lowercase();
        if db_info.encoding_label.is_empty() {
            //TODO need to check the default encoding of V1 and V2
            if db_info.version == ZdbVersion::V3 {
                db_info.encoding_label = "utf-8".to_string();
            }else{
                db_info.encoding_label = "utf-16le".to_string();
            }
        }
        db_info.is_utf16 = db_info.encoding_label.to_lowercase().starts_with("utf-16");

        let is_v1_v2_mdd = db_info.is_mdd && db_info.version != ZdbVersion::V3;
        //TODO need to check if we can get the "KeyCaseSensitive" and "StripKey" from the mdd file
        db_info.key_case_sensitive = get_node_attr_bool(&root_attrs,"KeyCaseSensitive", is_v1_v2_mdd); //Mdd file is case sensitive in v1 and v2
        db_info.strip_key = get_node_attr_bool(&root_attrs,"StripKey", !is_v1_v2_mdd); //Mdd file is not strip key in v1 and v2

        db_info._left_to_right = get_node_attr_bool(&root_attrs,"Left2Right", true);

        db_info.description = get_node_attr_str(&root_attrs,"Description");
        db_info.title = get_node_attr_str(&root_attrs,"Title");
        db_info.style_sheet = get_node_attr_str(&root_attrs,"StyleSheet");
        db_info.register_by = get_node_attr_str(&root_attrs,"RegisterBy");

        //To be compatible with old version which use Compat(typos) instead of Compact
        db_info.is_compact_format = get_node_attr_bool(&root_attrs,"Compat", false);
        if !db_info.is_compact_format {
            db_info.is_compact_format = get_node_attr_bool(&root_attrs,"Compact", db_info.is_compact_format);
        }

        if db_info.locale_id.is_empty() && !db_info.is_mdd {
            db_info.locale_id = generate_locale_id(&db_info.encoding_label, db_info.key_case_sensitive, db_info.strip_key);
        }

        Ok(db_info)
    }
}

#[derive(Clone,Debug)]
pub struct MetaUnit {
    pub db_info: DbInfo,
    pub crypto_key: Vec<u8>,
    pub content_data_total_length: u64,
    pub version: ZdbVersion,
    pub collator: Rc<UCollator>,
    pub encoding_obj: &'static Encoding,
    pub raw_header_xml:String,
}

fn read_cstr_with_crc<R: Read>(reader: &mut R) -> Result<String> {
    let length = reader.read_u32::<BigEndian>()?;
    let mut data = vec![0u8; length as usize];
    reader.read_exact(&mut data)?;
    let crc = reader.read_u32::<BigEndian>()?;
    if crc != adler::adler32_slice(&data).to_be() {
        return Err(ZdbError::crc_mismatch(crc, adler::adler32_slice(&data).to_be()));
    }
    if data.len()>1 { //if data is utf-16le, return utf-16le string
        if data[0]==b'<' && data[1]==0 { //ZDBV2 use utf-16le for header
            return decode_bytes_to_string(&data, &encoding_rs::UTF_16LE);
        }
    }
    return decode_bytes_to_string(&data, &encoding_rs::UTF_8);
}

impl MetaUnit {
    pub fn is_v1(&self) -> bool {
        self.version == ZdbVersion::V1
    }
    pub fn is_v2(&self) -> bool {
        self.version == ZdbVersion::V2
    }
    pub fn is_v3(&self) -> bool {
        self.version == ZdbVersion::V3
    }

    pub fn from_reader<R: Read + Seek>(reader: &mut R, device_id: &str, license_data: &str, content_data_total_length: u64) -> crate::Result<Self> {
        let raw_xml = read_cstr_with_crc(reader)?;
        //debug!("Zdb raw header:{}",raw_xml);
        let db_info: DbInfo = DbInfo::from_xml(&raw_xml)?;
        let version = db_info.version;
        let db_reg_code = if license_data.is_empty() { &db_info.embedded_reg_code } else { license_data };
        if db_reg_code.is_empty() && db_info.encryption_type.is_para_encrypted(){
            return Err(ZdbError::invalid_data_format("DB needs registration but no license data is provided"));
        }
    
        let crypto_key = if !db_reg_code.is_empty() {
            let encrypted_key = hex::decode(db_reg_code)
                .map_err(|e| ZdbError::invalid_data_format(format!("Failed to convert hex str:{}",e.to_string())))?;
            decrypt_salsa20(&encrypted_key, &ripemd_digest(device_id.as_bytes())?.as_slice())?
        } else {
            if version == ZdbVersion::V3 {
                fast_hash_digest(&db_info.uuid.as_bytes())?
            } else {
                vec![]
            }
        };

        let collator = UCollator::try_from(db_info.locale_id.as_str())?;
        Ok(Self { 
            crypto_key,
            encoding_obj: get_encoding_object_by_label(&db_info.encoding_label)?,
            db_info, 
            content_data_total_length,
            version,
            collator: Rc::new(collator),
            raw_header_xml: raw_xml,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_parsing() {
        let xml = r#"<ZDB GeneratedByEngineVersion="3.0" RequiredEngineVersion="3.0" ContentType="Html" RegisterBy="EMail" Description="" Title="" DefaultSortingLocale="zh-u-co-pinyin-ks-level1-ka-shifted-kr-space-punct-symbol-digit-en-others-hani" UUID="be335fe3-139b-4b28-8d48-a264d8fe7585" CreationDate="2024-4-20" Compact="No" DataSourceFormat="107" StyleSheet=""/>"#;
        
        match DbInfo::from_xml(xml) {
            Ok(db_info) => {
                assert_eq!(db_info.tag, "zdb");
                assert_eq!(db_info.version, ZdbVersion::V3);
                assert_eq!(db_info.content_type, ContentType::Html);
                assert_eq!(db_info.uuid, "be335fe3-139b-4b28-8d48-a264d8fe7585");
                assert_eq!(db_info.is_mdd, false);
                println!("XML parsing test passed!");
            }
            Err(e) => {
                panic!("XML parsing failed: {:?}", e);
            }
        }
    }
}
