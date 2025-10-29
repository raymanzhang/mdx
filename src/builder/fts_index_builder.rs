//! Full-text search index builder for MDX databases
//!
//! This module provides functionality to create, merge, and pack Tantivy full-text search
//! indexes for MDX database files.

use std::fs;
use std::io;
use std::path::PathBuf;

use log::{info, warn};
use tantivy::doc;
use tantivy::schema::{Field, Schema, INDEXED, STORED, TEXT};
use tantivy::{Index, TantivyDocument};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::storage::key_block::EntryNo;
use crate::readers::mdx_reader::MdxReader;
use crate::utils::progress_report::{ProgressReportFn, ProgressState};
use crate::{Result, ZdbError};

const MDICT_INDEX_EXT: &str = "idx";

pub struct IndexFields {
    pub entry_no: Field,
    pub key: Field,
    pub content: Field,
}

/// Create a new Tantivy index for MDX full-text search
fn init_index(index_dir_path: &PathBuf) -> Result<(Index, IndexFields)> {    
    if index_dir_path.exists() {
        fs::remove_dir_all(&index_dir_path)?;
    }

    // Create directory
    fs::create_dir_all(&index_dir_path)?;
    
    // Build schema
    let mut schema_builder = Schema::builder();
    
    // Add entry_no field (u64, stored and indexed)
    let entry_no = schema_builder.add_u64_field("entry_no", INDEXED | STORED);
    
    // Add key field (text, stored and indexed)
    let key = schema_builder.add_text_field("key", TEXT | STORED);
    
    // Add content field (text, indexed only - no storage since it's HTML content)
    let content = schema_builder.add_text_field("content", TEXT);
    
    let schema = schema_builder.build();
    
    // Create index in the specified directory
    let index = Index::create_in_dir(&index_dir_path, schema).map_err(|e| ZdbError::general_error(e.to_string()))?;
    
    let index_fields = IndexFields {
        entry_no,
        key,
        content,
    };
    
    Ok((index, index_fields))
}

/// Index an MDX database file into a Tantivy index using MdxReader
pub fn make_index(file_path: &PathBuf, prog_rpt: Option<ProgressReportFn>) -> Result<()> {
    info!("Indexing MDX file: {}", file_path.display());
    
    // Create URL from file path and open with MdxReader
    let mdx_url = url::Url::from_file_path(file_path)
        .map_err(|_| ZdbError::invalid_path(format!("{}", file_path.display())))?;
    
    let mut mdx_reader = MdxReader::from_url(&mdx_url, "")?;
    let entry_count = mdx_reader.get_entry_count();
    
    info!("Database contains {} entries", entry_count);
    
    let mut index_dir_path = file_path.clone();
    index_dir_path.set_extension("");
    
    // Create the Tantivy index  
    let (index, index_fields) = init_index(&index_dir_path)?;
    let mut index_writer = index.writer(50_000_000)
        .map_err(|e| ZdbError::general_error(format!("Failed to create index writer: {}", e)))?;
    
    info!("Created Tantivy index at: {}", index_dir_path.display());
    
    // Create progress state with 10% report interval
    let mut progress_state = ProgressState::new("FtsIndexBuilder::make_index", entry_count, 10, prog_rpt);
    
    // Index all entries
    for entry_no in 0..entry_count {
        // Get the index and content for this entry
        let key_index = mdx_reader.get_index(entry_no as EntryNo)?;
        
        let result = mdx_reader.get_html(&key_index);
        if let Err(e) = result {
            warn!("Failed to get HTML content for entry {}: {}", entry_no, e);
            continue;
        }
        let html_content = result.unwrap();
        
        // Extract text from HTML
        let text_content = crate::utils::utils::extract_text_from_html(&html_content)?;
        
        // Create document and add to index
        let doc = doc!(
            index_fields.entry_no => entry_no,
            index_fields.key => key_index.key.clone(),
            index_fields.content => text_content,
        );
        
        index_writer.add_document(doc)
            .map_err(|e| ZdbError::general_error(format!("Failed to add document: {}", e)))?;
        
        // Report progress and check for cancellation
        if progress_state.report(entry_no) {
            info!("Indexing cancelled by user");
            return Err(ZdbError::user_interrupted());
        }        
    }
    
    // Commit the index
    info!("Committing index...");
    index_writer.commit()
        .map_err(|e| ZdbError::general_error(format!("Failed to commit index: {}", e)))?;
    
    info!("Successfully indexed {} entries to Tantivy index", entry_count);
    
    drop(index_writer); // Drop the index writer to release the file lock
        
    // Merge index segments
    info!("Merging index segments...");
    let mut progress_state = ProgressState::new("FtsIndexBuilder::merge_index", 1, 10, prog_rpt);
    merge_index(&index_dir_path)?;
    if progress_state.report(1) {
        info!("Merge index cancelled by user");
        return Err(ZdbError::user_interrupted());
    }

    // Pack index into .idx file and remove source directory
    info!("Packing index into .{} file...", MDICT_INDEX_EXT);
    let mut progress_state = ProgressState::new("FtsIndexBuilder::pack_index", 1, 10, prog_rpt);
    pack_index(&index_dir_path, true)?;
    if progress_state.report(1) {
        info!("Pack index cancelled by user");
        return Err(ZdbError::user_interrupted());
    }
    
    info!("Index creation, optimization, and packing completed successfully");
    
    Ok(())
}

/// Merge index segments for optimization
pub fn merge_index(index_path: &PathBuf) -> Result<()> {
    info!("Starting index segment optimization...");
    
    // Open the existing index
    let index = Index::open_in_dir(index_path)
        .map_err(|e| ZdbError::general_error(format!("Failed to open index: {}", e)))?;
    
    // Create a writer with moderate heap to trigger merging while meeting minimum requirements
    let mut merge_writer: tantivy::IndexWriter<TantivyDocument> = index.writer(50_000_000) // 50MB - enough to trigger merging
        .map_err(|e| ZdbError::general_error(format!("Failed to create merge writer: {}", e)))?;

    // Get searchable segment IDs
    let segment_ids = index.searchable_segment_ids().map_err(|e| ZdbError::general_error(format!("Failed to get searchable segment IDs: {}", e)))?;
    if !segment_ids.is_empty() {
        merge_writer.merge(&segment_ids).wait()
            .map_err(|e| ZdbError::general_error(format!("Failed to merge segments: {}", e)))?;
        merge_writer.commit()
            .map_err(|e| ZdbError::general_error(format!("Failed to commit index: {}", e)))?;
    }
    info!("Merge segments: {:?}", segment_ids);    
    info!("Index segments merged successfully");
            
    Ok(())
}

/// Pack the index directory into a .idx file using ZIP format with stored compression
/// 
/// # Parameters
/// * `index_path` - Path to the index directory to pack
/// * `remove_source` - Whether to remove the source directory after packing
pub fn pack_index(index_path: &PathBuf, remove_source: bool) -> Result<()> {
    use walkdir::WalkDir;
    info!("Packing index directory (ZIP Stored): {}", index_path.display());
    if !index_path.exists() || !index_path.is_dir() {
        return Err(ZdbError::general_error(format!("Index directory does not exist: {}", index_path.display())));
    }

    // Output file keeps previous `.idx` extension for compatibility, but content is a ZIP archive.
    let zip_file_path = format!("{}.{}",index_path.display(),MDICT_INDEX_EXT);

    info!("Creating ZIP .{} file: {}", MDICT_INDEX_EXT, zip_file_path);

    let zip_file = fs::File::create(&zip_file_path)
        .map_err(|e| ZdbError::general_error(format!("Failed to create output file: {}", e)))?;
    let mut zip = ZipWriter::new(zip_file);
    let options = FileOptions::<()>::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    let base = index_path.canonicalize().map_err(|e| ZdbError::general_error(format!("Failed to resolve base path: {}", e)))?;

    for entry in WalkDir::new(&base) {
        let entry = entry.map_err(|e| ZdbError::general_error(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();
        if path == base.as_path() { continue; }

        let rel = path.strip_prefix(&base).map_err(|e| ZdbError::general_error(format!("Failed to calc relative path: {}", e)))?;
        let name = rel.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            // Ensure directory entry exists in zip
            let dir_name = if name.ends_with('/') { name.clone() } else { format!("{}/", name) };
            zip.add_directory(dir_name, FileOptions::<()>::default().unix_permissions(0o755))
                .map_err(|e| ZdbError::general_error(format!("Failed to add directory to zip: {}", e)))?;
        } else if path.is_file() {
            zip.start_file(name, options).map_err(|e| ZdbError::general_error(format!("Failed to start zip file entry: {}", e)))?;
            let mut f = fs::File::open(path).map_err(|e| ZdbError::general_error(format!("Failed to open file {}: {}", path.display(), e)))?;
            io::copy(&mut f, &mut zip).map_err(|e| ZdbError::general_error(format!("Failed to write file to zip: {}", e)))?;
        }
    }

    zip.finish().map_err(|e| ZdbError::general_error(format!("Failed to finalize zip: {}", e)))?;
    info!("Successfully packed index into ZIP (Stored) at: {}", zip_file_path);

    if remove_source {
        fs::remove_dir_all(index_path).map_err(|e| ZdbError::general_error(format!("Failed to remove original index directory: {}", e)))?;
        info!("Removed original index directory: {}", index_path.display());
    }

    Ok(())
}


