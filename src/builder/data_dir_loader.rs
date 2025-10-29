use std::collections::LinkedList;
use std::fs;
use std::path::{Path, PathBuf};

use crate::builder::data_loader::{DataLoader, ZdbRecord};
use crate::utils::io_utils::{scan_dir, windows_path_to_unix_path};
use crate::utils::progress_report::{ProgressReportFn, ProgressState};
use crate::{Result, ZdbError};

/// DataDirLoader is a data loader that loads data from a directory.
pub struct DataDirLoader{
    source_dir: String,
}

impl DataLoader for DataDirLoader{
    fn load_data(&mut self, entry: &ZdbRecord) -> Result<Vec<u8>> {
        let file_path = Path::new(&self.source_dir).join(&entry.content);
        let data = fs::read(&file_path)?;
        Ok(data)
    }
}



impl DataDirLoader{
    pub fn new(source_dir: &str, prog_rpt: Option<ProgressReportFn>) -> Result<(Self, Vec<ZdbRecord>)> {
         // Scan for all files in the directory
         let dir_path = Path::new(&source_dir);
         let mut files = LinkedList::<PathBuf>::new();
         let pattern = regex::Regex::new(r".*").unwrap(); // Match all files
         scan_dir(&dir_path, &pattern, true, &mut files)?; // recursive scan
         
         log::debug!("Found {} files to pack", files.len());
         let mut progress_state = ProgressState::new("DataDirLoader::new", files.len() as u64, 5, prog_rpt);

         let base_dir = dir_path.canonicalize()?;
         let mut entry_records = Vec::<ZdbRecord>::with_capacity(files.len());
         for (index, file_path) in files.iter().enumerate() {
             let relative_path = file_path.strip_prefix(&base_dir)
                 .map_err(|_| ZdbError::invalid_data_format(format!("Failed to create relative path: {}", file_path.display())))?;
             
             // Use forward slashes for MDD keys and prefix with backslash
             let key = format!("/{}", windows_path_to_unix_path(&relative_path.to_string_lossy()));
             
            let record = ZdbRecord {
                key: key.clone(),
                content_offset_in_source: 0, // Will be set later during building
                position: index as u64,
                content: file_path.to_string_lossy().to_string(), // Store file path in content field
                content_len: fs::metadata(&file_path)?.len(),
                line_no: 0, //unused for mdd
            };
             entry_records.push(record);
            if progress_state.report(index as u64) {
                return Err(ZdbError::user_interrupted());
            }
         }
         Ok((DataDirLoader{
            source_dir: source_dir.to_string(),
         }, entry_records))
    }
}