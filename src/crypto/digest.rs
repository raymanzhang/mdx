//! Hash digest functions for MDX dictionary files.
//!
//! This module provides cryptographic hash functions used for:
//! - Generating unique identifiers for dictionary files
//! - Creating checksums for data integrity
//! - Computing fast hashes for lookups
//!
//! # Examples
//!
//! ```
//! use mdx::digest::fast_hash_digest;
//!
//! let data = b"Hello, world!";
//! let hash = fast_hash_digest(data).unwrap();
//! assert_eq!(hash.len(), 16); // 128-bit hash
//! ```

use ripemd128::{Digest, Ripemd128};
use xxhash_rust::xxh64::Xxh64;

use crate::{Result, ZdbError};

/// Computes a 128-bit hash digest using two XXH64 hashes over the input.
///
/// This function splits the input into two parts and computes an XXH64 hash
/// for each part, then combines them into a 16-byte digest in big-endian format.
///
/// # Arguments
///
/// * `input` - The data to hash
///
/// # Returns
///
/// Returns a 16-byte Vec<u8> containing the hash digest.
///
/// # Errors
///
/// Returns an error if the input is empty.
///
/// # Examples
///
/// ```
/// use mdx::digest::fast_hash_digest;
///
/// let data = b"test data";
/// let hash = fast_hash_digest(data).unwrap();
/// assert_eq!(hash.len(), 16);
/// ```
pub fn fast_hash_digest(input: &[u8]) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Err(ZdbError::invalid_parameter("Input is empty"));
    }
    let mut output = Vec::with_capacity(16);

    // Calculate length of first part: ceiling of (input length + 1) / 2
    let first_part_len = (input.len() + 1) / 2;

    // Compute hash1 on first part using XXH64 with seed 0, store in network byte order
    let mut hasher1 = Xxh64::new(0);
    hasher1.update(&input[..first_part_len]);
    output.extend_from_slice( &hasher1.digest().to_be_bytes() ); // Convert to big-endian (network byte order)

    // Compute hash2 on second part if input length > 1, store in network byte order
    if input.len() > 1 {
        let mut hasher2 = Xxh64::new(0);
        hasher2.update(&input[first_part_len..]);
        output.extend_from_slice(&hasher2.digest().to_be_bytes()); // Convert to big-endian (network byte order)
    }

    Ok(output)
}

/// Computes a RIPEMD-128 hash digest of the input data.
///
/// # Arguments
///
/// * `data` - The data to hash
///
/// # Returns
///
/// Returns a 16-byte Vec<u8> containing the RIPEMD-128 hash.
pub fn ripemd_digest(data: &[u8]) -> Result<Vec<u8>> {
    let mut ripemd = Ripemd128::new();
    ripemd.input(data);
    let digest = ripemd.result();
    Ok(digest.to_vec())
}
