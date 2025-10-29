//! Compression and decompression support for ZDB files.
//!
//! This module provides a unified interface for multiple compression algorithms
//! used in MDX/MDD dictionary files. It supports:
//! - No compression
//! - LZO compression
//! - Deflate (zlib) compression
//! - LZMA compression
//! - Bzip2 compression
//! - LZ4 compression

use std::io::{Read, Write};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use crate::{ZdbError, Result};

/// Compression methods supported by ZDB files.
///
/// Each variant corresponds to a specific compression algorithm that can be
/// used for compressing dictionary data blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CompressionMethod {
    /// No compression
    None = 0,
    /// LZO compression (fast, moderate compression ratio)
    Lzo = 1,
    /// Deflate/zlib compression (default, good balance)
    #[default]
    Deflate = 2,
    /// LZMA compression (slow, high compression ratio)
    Lzma = 3,
    /// Bzip2 compression (moderate speed, good compression)
    Bzip2 = 4,
    /// LZ4 compression (very fast, moderate compression)
    Lz4 = 5,
}

impl TryFrom<u8> for CompressionMethod {
    type Error = ZdbError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(CompressionMethod::None),
            1 => Ok(CompressionMethod::Lzo),
            2 => Ok(CompressionMethod::Deflate),
            3 => Ok(CompressionMethod::Lzma),
            4 => Ok(CompressionMethod::Bzip2),
            5 => Ok(CompressionMethod::Lz4),
            _ => Err(ZdbError::invalid_parameter(format!("Invalid compression method:{}",value))),
        }
    }
}

/// Common interface for compression and decompression operations.
///
/// All compression algorithms implement this trait to provide a uniform API.
pub trait Compressor {
    /// Compresses the input data.
    ///
    /// # Arguments
    ///
    /// * `data` - The raw data to compress
    ///
    /// # Returns
    ///
    /// Returns the compressed data.
    ///
    /// # Errors
    ///
    /// Returns an error if compression fails.
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;
    
    /// Decompresses the input data.
    ///
    /// # Arguments
    ///
    /// * `data` - The compressed data
    /// * `original_size` - The expected size of the decompressed data
    ///
    /// # Returns
    ///
    /// Returns the decompressed data.
    ///
    /// # Errors
    ///
    /// Returns an error if decompression fails or the output size doesn't match.
    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>>;
}

/// No-op compressor that passes data through unchanged.
pub struct NoCompression;

impl Compressor for NoCompression {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn decompress(&self, data: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

/// LZO compression implementation.
pub struct LzoCompressor;

impl Compressor for LzoCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut compressed = Vec::with_capacity(data.len());
        let mut ctx = rust_lzo::LZOContext::new();
        let error = ctx.compress(data, &mut compressed);
        match error {
            rust_lzo::LZOError::OK => Ok(compressed),
            _ => Err(ZdbError::compression_error(format!("LZO compression error: {}", error as u32))),
        }
    }

    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        let mut decompressed = vec![0; original_size];
        let (result, error) = rust_lzo::LZOContext::decompress_to_slice(data, &mut decompressed);
        if error!=rust_lzo::LZOError::OK {
            return Err(ZdbError::decompression_error(format!("LZO decompression error: {}",error as u32)));
        }
        Ok(result.to_vec())
    }
}

/// Deflate (zlib) compression implementation.
pub struct DeflateCompressor;

impl Compressor for DeflateCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)
            .map_err(|e| ZdbError::compression_error(format!("Deflate error: {}", e)))?;
        Ok(encoder.finish()?)
    }

    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        let mut decoder = ZlibDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| ZdbError::decompression_error(format!("Inflate error: {}", e)))?;
        if decompressed.len() != original_size {
            return Err(ZdbError::decompression_error(format!("expected size {} but got {}", original_size, decompressed.len())));
        }
        Ok(decompressed)
    }
}

/// LZMA compression implementation.
pub struct LzmaCompressor;

impl Compressor for LzmaCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut compressed = Vec::new();
        lzma_rs::lzma_compress(&mut std::io::Cursor::new(data), &mut compressed)
            .map_err(|e| ZdbError::compression_error(format!("Lzma Err:{}", e)))?;
        Ok(compressed)
    }

    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        let mut decompressed = Vec::with_capacity(original_size);
        lzma_rs::lzma_decompress(&mut std::io::Cursor::new(data), &mut decompressed)
            .map_err(|e| ZdbError::decompression_error(format!("Lzma Err:{}", e)))?;
        Ok(decompressed)
    }
}

pub struct Bzip2Compressor;

impl Compressor for Bzip2Compressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        encoder.write_all(data)
            .map_err(|e| ZdbError::compression_error(format!("Bzip2 Err:{}", e)))?;
        Ok(encoder.finish()?)
    }

    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        let mut decoder = bzip2::read::BzDecoder::new(data);
        let mut decompressed = vec![0; original_size];
        decoder.read_exact(&mut decompressed)
            .map_err(|e| ZdbError::decompression_error(format!("Bzip2 Err:{}", e)))?;
        Ok(decompressed)
    }
}

pub struct Lz4Compressor;

impl Compressor for Lz4Compressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut compressed = Vec::new();
        let mut encoder = lz4::EncoderBuilder::new().build(&mut compressed)?;
        encoder.write_all(data)
            .map_err(|e| ZdbError::compression_error(format!("Lz4 Err:{}", e)))?;
        let (_, result) = encoder.finish();
        result?;
        Ok(compressed)
    }

    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        let mut decompressed = vec![0; original_size];
        let mut decoder = lz4::Decoder::new(data)?;
        decoder.read_exact(&mut decompressed)
            .map_err(|e| ZdbError::decompression_error(format!("Lz4 Err:{}", e)))?;
    Ok(decompressed)
    }
}

pub fn get_compressor(method: CompressionMethod) -> Box<dyn Compressor> {
    match method {
        CompressionMethod::None => Box::new(NoCompression),
        CompressionMethod::Lzo => Box::new(LzoCompressor),
        CompressionMethod::Deflate => Box::new(DeflateCompressor),
        CompressionMethod::Lzma => Box::new(LzmaCompressor),
        CompressionMethod::Bzip2 => Box::new(Bzip2Compressor),
        CompressionMethod::Lz4 => Box::new(Lz4Compressor),
    }
} 