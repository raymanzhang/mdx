use std::fs::File;
use std::io::BufReader;

use crate::builder::data_loader::{DataLoader, ZdbRecord};
use crate::utils::io_utils::windows_path_to_unix_path;
use crate::storage::key_block::EntryNo;
use crate::readers::mdx_reader::MdxReader;
use crate::utils::progress_report::{ProgressReportFn, ProgressState};
use crate::readers::zdb_reader::ZdbReader;
use crate::{Result, ZdbError};


pub struct ZdbLoader{
    pub input_reader: ZdbReader<BufReader<File>>,
    compact_stylesheet: Vec<(String, String)>,
}

impl DataLoader for ZdbLoader{

    fn load_data(&mut self, entry: &ZdbRecord) -> Result<Vec<u8>> {
        let key_index = self.input_reader.get_index(entry.position as EntryNo)?;
        if !self.input_reader.meta.db_info.is_mdd {
            let content =self.input_reader.get_string(&key_index, false)?;
            if self.compact_stylesheet.is_empty(){
                return Ok(content.into_bytes());
            }else{
                let expanded_content = MdxReader::reformat(&content, &self.compact_stylesheet)?;
                return Ok(expanded_content.into_bytes());    
            }
        } else {
            self.input_reader.get_data(&key_index, false)
        }
    }
}



impl ZdbLoader{
    pub fn new(source_file:&str, device_id:&str, license_key:&str, prog_rpt: Option<ProgressReportFn>) -> Result<(Self, Vec<ZdbRecord>)> {
        let mut zdb_reader = ZdbReader::<BufReader<File>>::from_file(source_file, device_id, license_key)?;
        let mut entry_records = Vec::<ZdbRecord>::with_capacity(zdb_reader.get_entry_count() as usize);
        let mut progress_state = ProgressState::new("ZdbLoader::new",zdb_reader.get_entry_count() as u64, 5, prog_rpt);
        let compact_stylesheet = MdxReader::load_compact_stylesheet(&zdb_reader.meta.db_info.style_sheet)?;
    
        let mut i=0u64;
        while i < zdb_reader.get_entry_count() {
            let key_index = zdb_reader.get_index(i as EntryNo)?;
            let rec = ZdbRecord {
                key: if zdb_reader.meta.db_info.is_mdd && key_index.key.starts_with("\\") {
                    windows_path_to_unix_path(&key_index.key)
                } else {
                    key_index.key.clone()
                },
                content_offset_in_source: 0, //offset should be calculated after the data is sorted by sort_key
                position: i, //position is the entry no for zdb
                content: String::new(), //unused for zdb    
                content_len: zdb_reader.get_content_length(i as EntryNo)?, //probably need to be re-calculated again later due to encoding changes
                line_no: 0, //unused for zdb
            };
            entry_records.push(rec);
            i += 1;
    
            if progress_state.report(i) {
                return Err(ZdbError::user_interrupted());
            }
        }
        Ok((ZdbLoader{
            input_reader: zdb_reader,
            compact_stylesheet,
        }, entry_records))
    }
}