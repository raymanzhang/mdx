//! Encryption and decryption support for ZDB files.
//!
//! This module provides encryption methods for protecting dictionary data:
//! - No encryption (plain text)
//! - Simple XOR-based encryption
//! - Salsa20 stream cipher encryption (default)
//!
//! # Examples
//!
//! ```
//! use mdx::encryption::{SimpleEncryptor, Encryptor};
//!
//! let key = b"my_secret_key";
//! let nonce = b"nonce_value";
//! let mut encryptor = SimpleEncryptor::new(key, nonce);
//!
//! let plaintext = b"Hello, World!";
//! let mut ciphertext = vec![0u8; plaintext.len()];
//! let mut decrypted = vec![0u8; plaintext.len()];
//!
//! encryptor.encrypt(plaintext, &mut ciphertext).unwrap();
//! encryptor.decrypt(&ciphertext, &mut decrypted).unwrap();
//! assert_eq!(plaintext, &decrypted[..]);
//! ```

use std::io;

use super::salsa20::*;
use crate::{Result, ZdbError};

/// Encryption methods supported by ZDB files.
///
/// Each variant corresponds to a specific encryption algorithm that can be
/// used for encrypting dictionary data blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EncryptionMethod {
    /// No encryption
    None = 0,
    /// Simple XOR-based encryption
    Simple = 1,
    /// Salsa20 stream cipher (default, recommended)
    #[default]
    Salsa20 = 2,
}

impl TryFrom<u8> for EncryptionMethod {
    type Error = ZdbError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(EncryptionMethod::None),
            1 => Ok(EncryptionMethod::Simple),
            2 => Ok(EncryptionMethod::Salsa20),
            _ => Err(ZdbError::invalid_parameter(format!("Invalid encryption method:{}",value))),
        }
    }
}

/// Common interface for encryption and decryption operations.
///
/// All encryption algorithms implement this trait to provide a uniform API.
pub trait Encryptor {
    /// Encrypts the input data into the output buffer.
    ///
    /// # Arguments
    ///
    /// * `input` - The plaintext data to encrypt
    /// * `output` - The buffer to write encrypted data to (must be same length as input)
    ///
    /// # Errors
    ///
    /// Returns an error if the input and output lengths don't match or encryption fails.
    fn encrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()>;
    
    /// Decrypts the input data into the output buffer.
    ///
    /// # Arguments
    ///
    /// * `input` - The encrypted data to decrypt
    /// * `output` - The buffer to write decrypted data to (must be same length as input)
    ///
    /// # Errors
    ///
    /// Returns an error if the input and output lengths don't match or decryption fails.
    fn decrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()>;
}

/// No-op encryptor that passes data through unchanged.
pub struct NoEncryption{
}

impl Encryptor for NoEncryption {

    fn encrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        if input.len() != output.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input and output length mismatch"));
        }
        output.copy_from_slice(input);
        Ok(())
    }

    fn decrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        if input.len() != output.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input and output length mismatch"));
        }
        output.copy_from_slice(input);
        Ok(())
    }
}

/// A simple XOR-based encryptor.
///
/// # Examples
///
/// ```
/// use mdx::encryption::SimpleEncryptor;
/// use mdx::encryption::Encryptor;
/// let key = b"secret";
/// let nonce = b"ignored";
/// let mut encryptor = SimpleEncryptor::new(key, nonce);
/// let input = b"hello world";
/// let mut encrypted = vec![0u8; input.len()];
/// let mut decrypted = vec![0u8; input.len()];
///
/// // Encrypt
/// encryptor.encrypt(input, &mut encrypted).unwrap();
/// // Decrypt with decrypt()
/// encryptor.decrypt(&encrypted, &mut decrypted).unwrap();
/// assert_eq!(input, &decrypted[..]);
///
/// // Decrypt with inplace_decrypt()
/// let mut encrypted2 = encrypted.clone();
/// encryptor.inplace_decrypt(&mut encrypted2).unwrap();
/// assert_eq!(input, &encrypted2[..]);
/// ```
pub struct SimpleEncryptor{
    key: Vec<u8>,
}

impl SimpleEncryptor {
    pub fn new(key: &[u8], _nonce: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }
}

impl Encryptor for SimpleEncryptor {
    fn encrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        if input.len() != output.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input and output length mismatch"));
        }
        let key_len = self.key.len();
        let mut last_byte = 0x36;
        for (i, (&in_byte, out_byte)) in input.iter().zip(output.iter_mut()).enumerate() {
            let b = in_byte ^ self.key[i % key_len] ^ (i as u8) ^ last_byte;
            last_byte = ((b & 0x0f) << 4) | ((b & 0xf0) >> 4);
            *out_byte = last_byte;
        }
        Ok(())
    }

    fn decrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        if input.len() != output.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Input and output length mismatch"));
        }
        let key_len = self.key.len();
        let mut last_byte = 0x36;
        for (i, (&in_byte, out_byte)) in input.iter().zip(output.iter_mut()).enumerate() {
            let b = in_byte;
            *out_byte = (((b & 0x0f) << 4) | ((b & 0xf0) >> 4)) ^ self.key[i % key_len] ^ (i as u8) ^ last_byte;
            last_byte = b;
        }
        Ok(())
    }
}
impl SimpleEncryptor {
    pub fn inplace_decrypt(&mut self, input: &mut [u8]) -> io::Result<()> {
        let key_len = self.key.len();
        let mut last_byte = 0x36;
        let mut i = 0;
        while i < input.len() {
            let b = input[i];
            input[i] = (((b & 0x0f) << 4) | ((b & 0xf0) >> 4)) ^ self.key[i % key_len] ^ (i as u8) ^ last_byte;
            last_byte = b;
            i += 1;
        }
        Ok(())
    }
}

pub struct Salsa20Encryptor {
    ctx: Salsa20Context,
}

impl Salsa20Encryptor{
    pub fn new(key: &[u8], nonce: &[u8]) -> Self {
        let mut ctx = Salsa20Context { input: [0u32; 16] };
        salsa20_key_setup(&mut ctx, key, 128);
        salsa20_iv_setup(&mut ctx, nonce);
        Self { ctx }
    }

}
impl Encryptor for Salsa20Encryptor {
    fn encrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        salsa20_encrypt_bytes(&mut self.ctx, input, output);
        Ok(())
    }

    fn decrypt(&mut self, input: &[u8], output: &mut [u8]) -> io::Result<()> {
        salsa20_decrypt_bytes(&mut self.ctx, input, output);
        Ok(())
    }
}

pub fn get_encryptor(method: EncryptionMethod, key: &[u8], nonce: &[u8]) -> Result<Box<dyn Encryptor>> {
    let encryptor:Box<dyn Encryptor> = match method {
        EncryptionMethod::None => Box::new(NoEncryption{}),
        EncryptionMethod::Simple => Box::new(SimpleEncryptor::new(key, nonce)),
        EncryptionMethod::Salsa20 => Box::new(Salsa20Encryptor::new(key, nonce)),
    };
    Ok(encryptor)
} 

pub fn decrypt_salsa20(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    let nonce = [0u8; 8];
    let mut salsa20_encryptor =
        get_encryptor(crate::crypto::encryption::EncryptionMethod::Salsa20, key, &nonce)?;
    let mut decrypted_data = vec![0; data.len()];
    salsa20_encryptor.decrypt(data, &mut decrypted_data)?;
    Ok(decrypted_data)
}

pub fn encrypt_salsa20(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    let nonce = [0u8; 8];
    let mut salsa20_encryptor =
        get_encryptor(crate::crypto::encryption::EncryptionMethod::Salsa20, key, &nonce)?;
    let mut encrypted_data = vec![0; data.len()];
    salsa20_encryptor.encrypt(data, &mut encrypted_data)?;
    Ok(encrypted_data)
}