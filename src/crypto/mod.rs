// Cryptographic utilities for ZDB files
//
// This module provides cryptographic operations for handling encrypted dictionary data.

pub mod digest;
pub mod encryption;
pub mod salsa20;

pub use digest::ripemd_digest;
pub use encryption::{EncryptionMethod, get_encryptor, decrypt_salsa20, encrypt_salsa20};
