use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tantivy::directory::error::LockError;
use tantivy::directory::{self, DirectoryLock, FileHandle, Lock};

use crate::error::{Result, ZdbError};

// ZIP entry metadata for direct file access
#[derive(Debug, Clone)]
struct ZipEntryInfo {
    offset: u64,
    size: u64,
}

// Cache to store entry information 
type EntryCache = Arc<Mutex<Option<HashMap<String, ZipEntryInfo>>>>;

#[derive(Clone, Debug)]
pub struct ZipDirectory {
    zip_path: PathBuf,
    entry_cache: EntryCache,
}

impl ZipDirectory {
    pub fn open(zip_path: PathBuf) -> Self { 
        Self { 
            zip_path,
            entry_cache: Arc::new(Mutex::new(None)),
        } 
    }

    fn ensure_cache_loaded(&self) -> Result<()> {
        let mut cache = self.entry_cache.lock().unwrap();
        if cache.is_some() {
            return Ok(());
        }

        let file = fs::File::open(&self.zip_path)
            .map_err(|e| ZdbError::general_error(format!("Failed to open zip: {}", e)))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| ZdbError::general_error(format!("Failed to read zip: {}", e)))?;
        
        let mut entries = HashMap::new();
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                if !entry.is_dir() && entry.compression() == zip::CompressionMethod::Stored {
                    // Only support uncompressed entries for direct access
                    let name = entry.name().to_string();
                    let info = ZipEntryInfo {
                        offset: entry.data_start(),
                        size: entry.size(),
                    };
                    entries.insert(name, info);
                }
            }
        }
        
        *cache = Some(entries);
        Ok(())
    }

    fn get_entry_info(&self, path: &std::path::Path) -> Result<ZipEntryInfo> {
        self.ensure_cache_loaded()?;
        let cache = self.entry_cache.lock().unwrap();
        let entries = cache.as_ref().unwrap();
        let name = path.to_string_lossy().replace('\\', "/");
        
        entries.get(&name)
            .cloned()
            .ok_or_else(|| ZdbError::general_error(format!("Entry not found in zip: {}", name)))
    }

    fn has_entry(&self, path: &std::path::Path) -> io::Result<bool> {
        match self.ensure_cache_loaded() {
            Ok(()) => {
                let cache = self.entry_cache.lock().unwrap();
                let entries = cache.as_ref().unwrap();
                let name = path.to_string_lossy().replace('\\', "/");
                Ok(entries.contains_key(&name))
            }
            Err(_) => Ok(false)
        }
    }
}

#[derive(Debug)]
struct ZipFileHandle {
    zip_path: PathBuf,
    entry_info: ZipEntryInfo,
}

impl ZipFileHandle {
    fn new(zip_path: PathBuf, entry_info: ZipEntryInfo) -> Self {
        Self { zip_path, entry_info }
    }
}

impl FileHandle for ZipFileHandle {
    fn read_bytes(&self, range: std::ops::Range<usize>) -> io::Result<directory::OwnedBytes> {
        use std::io::{Read, Seek, SeekFrom};
        
        if range.end > self.entry_info.size as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Range exceeds file size"));
        }
        
        let mut file = fs::File::open(&self.zip_path)?;
        file.seek(SeekFrom::Start(self.entry_info.offset + range.start as u64))?;
        
        let len = range.end - range.start;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer)?;
        
        let owned_bytes = directory::OwnedBytes::new(buffer);
        Ok(owned_bytes)
    }
}

impl tantivy::HasLen for ZipFileHandle {
    fn len(&self) -> usize {
        self.entry_info.size as usize
    }
}

impl directory::Directory for ZipDirectory {
    fn get_file_handle(&self, path: &std::path::Path) -> std::result::Result<std::sync::Arc<dyn directory::FileHandle>, directory::error::OpenReadError> {
        let entry_info = self.get_entry_info(path)
            .map_err(|e| directory::error::OpenReadError::wrap_io_error(
                io::Error::new(io::ErrorKind::NotFound, e.to_string()), 
                path.to_path_buf()
            ))?;
        
        let handle = ZipFileHandle::new(self.zip_path.clone(), entry_info);
        Ok(Arc::new(handle))
    }

    fn delete(&self, path: &std::path::Path) -> std::result::Result<(), directory::error::DeleteError> {
        Err(directory::error::DeleteError::IoError { 
            io_error: std::sync::Arc::new(io::Error::new(io::ErrorKind::Unsupported, "ZipDirectory is read-only")), 
            filepath: path.to_path_buf() 
        })
    }

    fn exists(&self, path: &std::path::Path) -> std::result::Result<bool, directory::error::OpenReadError> {
        self.has_entry(path).map_err(|e| directory::error::OpenReadError::wrap_io_error(e, path.to_path_buf()))
    }

    fn open_write(&self, path: &std::path::Path) -> std::result::Result<directory::WritePtr, directory::error::OpenWriteError> {
        Err(directory::error::OpenWriteError::IoError { 
            io_error: std::sync::Arc::new(io::Error::new(io::ErrorKind::Unsupported, "ZipDirectory is read-only")), 
            filepath: path.to_path_buf() 
        })
    }

    fn atomic_read(&self, path: &std::path::Path) -> std::result::Result<Vec<u8>, directory::error::OpenReadError> {
        let handle = self.get_file_handle(path)?;
        let len = handle.len();
        let owned_bytes = handle.read_bytes(0..len)
            .map_err(|e| directory::error::OpenReadError::wrap_io_error(e, path.to_path_buf()))?;
        Ok(owned_bytes.as_slice().to_vec())
    }

    fn atomic_write(&self, path: &std::path::Path, _data: &[u8]) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, format!("ZipDirectory is read-only: {}", path.display())))
    }

    fn sync_directory(&self) -> io::Result<()> { Ok(()) }

    fn watch(&self, watch_callback: directory::WatchCallback) -> tantivy::Result<directory::WatchHandle> {
        let _ = watch_callback; // no-op
        Ok(directory::WatchHandle::empty())
    }

    fn acquire_lock(&self, _lock: &Lock) -> std::result::Result<DirectoryLock, LockError> {
        // For read-only ZipDirectory, we return a dummy lock since there's no write operations
        // that could interfere with reading
        Ok(DirectoryLock::from(Box::new(())))
    }
}
