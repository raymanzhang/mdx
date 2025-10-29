use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use snafu::Backtrace;

use crate::builder::data_loader::{DataLoader, ZdbRecord, MAX_ENTRY_LEN, ZDB_MAX_KEYWORD_LENGTH};
use crate::utils::progress_report::{ProgressReportFn, ProgressState};
use crate::{Result, ZdbError};


pub struct MDictSourceLoader{
    pub source_file: String,
    input_reader: BufReader<File>,

}

fn skip_utf8_bom(line: &str) -> &str {
    // UTF-8 BOM is 0xEF 0xBB 0xBF which appears as \u{FEFF} in UTF-8
    line.strip_prefix('\u{FEFF}').unwrap_or(line)
}

fn is_text_end(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "</>" || trimmed.is_empty()
}

impl DataLoader for MDictSourceLoader{

    fn load_data(&mut self, entry: &ZdbRecord) -> Result<Vec<u8>> {
        let mut data = Vec::<u8>::with_capacity(entry.content_len as usize);
        self.input_reader.seek(SeekFrom::Start(entry.position))?;
        self.input_reader.read_exact(&mut data)?;
        Ok(data)
    }
}



impl MDictSourceLoader{
    pub fn new(source_file:&str, prog_rpt: Option<ProgressReportFn>) -> Result<(Self, Vec<ZdbRecord>)> {
        let source_file = source_file.to_string();
        let mut input_reader = BufReader::new(File::open(&source_file)?);
        // Get total file size for progress reporting
        let total_size = input_reader.seek(SeekFrom::End(0))?;
        input_reader.seek(SeekFrom::Start(0))?;
        
        let mut line_buffer = String::new();
        let mut line_count = 0usize;
        let mut entry_records = Vec::<ZdbRecord>::with_capacity(total_size as usize/1024);
        let mut progress_state = ProgressState::new("MDictSourceLoader::new", total_size, 10, prog_rpt);
    
        while !input_reader.fill_buf()?.is_empty() {
            line_buffer.clear();
            
            // Use standard library read_line
            input_reader.read_line(&mut line_buffer)?;
            line_count += 1;
            
            let mut line = line_buffer.as_str();
            if line_count == 1 {
                line = skip_utf8_bom(line);
            }
            
            let trimmed_line = line.trim_end_matches(&['\r', '\n']);        
            if trimmed_line.is_empty() {
                if input_reader.fill_buf()?.is_empty() { //This is the last line of the file, so we can break
                    break;
                } else {
                    return Err(ZdbError::InvalidDataFormat { 
                        message: "Invalid key".to_string(),
                        backtrace: Backtrace::capture(),
                    });
                }
            }
            if trimmed_line.len() > ZDB_MAX_KEYWORD_LENGTH {
                return Err(ZdbError::InvalidDataFormat { 
                    message: "Key too long".to_string(),
                    backtrace: Backtrace::capture(),
                });
            }
            
            // Record position where content starts (after the key line)
            let content_start_pos = input_reader.stream_position()?;
            
            // Read content until text end marker
            let mut content_buffer = String::new();
            
            loop {
                content_buffer.clear();
                
                let bytes_read = input_reader.read_line(&mut content_buffer)?;
                line_count += 1;
                if bytes_read == 0 || is_text_end(&content_buffer) {
                    break;
                }
                
            }
            let content_length = input_reader.stream_position()? - content_start_pos;
                
            if content_length > MAX_ENTRY_LEN as u64 {
                return Err(ZdbError::InvalidDataFormat { 
                    message: "Record too long".to_string(),
                    backtrace: Backtrace::capture(),
                });
            }
        
            // Create new record using ZdbRecord structure
            let record = ZdbRecord {
                key: trimmed_line.to_string(),
                content_offset_in_source: 0, // Will be set later during processing
                position: content_start_pos,
                content: String::new(), // Content will be loaded separately when needed
                content_len: content_length,
                line_no: line_count as u64,
            };
            
            entry_records.push(record);
            
            // Report progress using current file position
            let current_file_pos = input_reader.stream_position()?;
            if progress_state.report(current_file_pos) {
                return Err(ZdbError::user_interrupted());
            }
        }
        Ok((MDictSourceLoader{
            source_file,
            input_reader,
        }, entry_records))    
    }
}