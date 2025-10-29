//! Salsa20 stream cipher implementation.
//!
//! This module provides an implementation of the Salsa20 stream cipher,
//! translated from the reference implementation by D. J. Bernstein (public domain).
//! Salsa20 is a fast and secure stream cipher used for encrypting dictionary data.

// Salsa20 implementation in Rust, translated from salsa20-ref.c (Public domain by D. J. Bernstein)

/// Salsa20 cipher context containing the cipher state.
#[derive(Clone)]
pub struct Salsa20Context {
    /// Internal cipher state
    pub input: [u32; 16],
}

// Helper functions replacing C macros

/// Rotates a 32-bit value left by c bits.
#[inline]
fn rotate(v: u32, c: u32) -> u32 {
    v.rotate_left(c)
}

/// XOR operation on two 32-bit values.
#[inline]
fn xor(v: u32, w: u32) -> u32 {
    v ^ w
}

/// Addition operation on two 32-bit values (wrapping).
#[inline]
fn plus(v: u32, w: u32) -> u32 {
    v.wrapping_add(w)
}

/// Adds 1 to a 32-bit value (wrapping).
#[inline]
fn plus_one(v: u32) -> u32 {
    plus(v, 1)
}

/// Converts a 32-bit value to little-endian bytes.
#[inline]
fn u32_to_u8_little(output: &mut [u8], input: u32) {
    output[0] = input as u8;
    output[1] = (input >> 8) as u8;
    output[2] = (input >> 16) as u8;
    output[3] = (input >> 24) as u8;
}

/// Converts little-endian bytes to a 32-bit value.
#[inline]
fn u8_to_u32_little(input: &[u8]) -> u32 {
    u32::from_le_bytes(input[..4].try_into().unwrap())
}

fn salsa20_word_to_byte(output: &mut [u8; 64], input: &[u32; 16]) {
    let mut x = *input;

    for _ in (0..8).step_by(2) {
        x[4] = xor(x[4], rotate(plus(x[0], x[12]), 7));
        x[8] = xor(x[8], rotate(plus(x[4], x[0]), 9));
        x[12] = xor(x[12], rotate(plus(x[8], x[4]), 13));
        x[0] = xor(x[0], rotate(plus(x[12], x[8]), 18));
        x[9] = xor(x[9], rotate(plus(x[5], x[1]), 7));
        x[13] = xor(x[13], rotate(plus(x[9], x[5]), 9));
        x[1] = xor(x[1], rotate(plus(x[13], x[9]), 13));
        x[5] = xor(x[5], rotate(plus(x[1], x[13]), 18));
        x[14] = xor(x[14], rotate(plus(x[10], x[6]), 7));
        x[2] = xor(x[2], rotate(plus(x[14], x[10]), 9));
        x[6] = xor(x[6], rotate(plus(x[2], x[14]), 13));
        x[10] = xor(x[10], rotate(plus(x[6], x[2]), 18));
        x[3] = xor(x[3], rotate(plus(x[15], x[11]), 7));
        x[7] = xor(x[7], rotate(plus(x[3], x[15]), 9));
        x[11] = xor(x[11], rotate(plus(x[7], x[3]), 13));
        x[15] = xor(x[15], rotate(plus(x[11], x[7]), 18));

        x[1] = xor(x[1], rotate(plus(x[0], x[3]), 7));
        x[2] = xor(x[2], rotate(plus(x[1], x[0]), 9));
        x[3] = xor(x[3], rotate(plus(x[2], x[1]), 13));
        x[0] = xor(x[0], rotate(plus(x[3], x[2]), 18));
        x[6] = xor(x[6], rotate(plus(x[5], x[4]), 7));
        x[7] = xor(x[7], rotate(plus(x[6], x[5]), 9));
        x[4] = xor(x[4], rotate(plus(x[7], x[6]), 13));
        x[5] = xor(x[5], rotate(plus(x[4], x[7]), 18));
        x[11] = xor(x[11], rotate(plus(x[10], x[9]), 7));
        x[8] = xor(x[8], rotate(plus(x[11], x[10]), 9));
        x[9] = xor(x[9], rotate(plus(x[8], x[11]), 13));
        x[10] = xor(x[10], rotate(plus(x[9], x[8]), 18));
        x[12] = xor(x[12], rotate(plus(x[15], x[14]), 7));
        x[13] = xor(x[13], rotate(plus(x[12], x[15]), 9));
        x[14] = xor(x[14], rotate(plus(x[13], x[12]), 13));
        x[15] = xor(x[15], rotate(plus(x[14], x[13]), 18));
    }

    for i in 0..16 {
        x[i] = plus(x[i], input[i]);
        u32_to_u8_little(&mut output[i * 4..i * 4 + 4], x[i]);
    }
}

pub fn salsa20_key_setup(ctx: &mut Salsa20Context, key: &[u8], kbits: u32) {
    assert!(kbits == 128 || kbits == 256, "Key size must be 128 or 256 bits");

    let sigma = b"expand 32-byte k";
    let tau = b"expand 16-byte k";
    let constants = if kbits == 256 { sigma } else { tau };

    let key_len = (kbits / 8) as usize;
    assert!(key.len() >= key_len, "Key buffer too small");

    ctx.input[1] = u8_to_u32_little(&key[0..4]);
    ctx.input[2] = u8_to_u32_little(&key[4..8]);
    ctx.input[3] = u8_to_u32_little(&key[8..12]);
    ctx.input[4] = u8_to_u32_little(&key[12..16]);

    let k_offset = if kbits == 256 { 16 } else { 0 };
    ctx.input[11] = u8_to_u32_little(&key[k_offset..k_offset + 4]);
    ctx.input[12] = u8_to_u32_little(&key[k_offset + 4..k_offset + 8]);
    ctx.input[13] = u8_to_u32_little(&key[k_offset + 8..k_offset + 12]);
    ctx.input[14] = u8_to_u32_little(&key[k_offset + 12..k_offset + 16]);

    ctx.input[0] = u8_to_u32_little(&constants[0..4]);
    ctx.input[5] = u8_to_u32_little(&constants[4..8]);
    ctx.input[10] = u8_to_u32_little(&constants[8..12]);
    ctx.input[15] = u8_to_u32_little(&constants[12..16]);
}

pub fn salsa20_iv_setup(ctx: &mut Salsa20Context, iv: &[u8]) {
    assert!(iv.len() >= 8, "IV must be at least 8 bytes");

    ctx.input[6] = u8_to_u32_little(&iv[0..4]);
    ctx.input[7] = u8_to_u32_little(&iv[4..8]);
    ctx.input[8] = 0;
    ctx.input[9] = 0;
}

pub fn salsa20_encrypt_bytes(ctx: &mut Salsa20Context, m: &[u8], c: &mut [u8]) {
    assert!(m.len() == c.len(), "Input and output buffers must have same length");

    if m.is_empty() {
        return;
    }

    let mut output = [0u8; 64];
    let mut bytes = m.len();
    let mut m_offset = 0;
    let mut c_offset = 0;

    loop {
        salsa20_word_to_byte(&mut output, &ctx.input);
        ctx.input[8] = plus_one(ctx.input[8]);
        if ctx.input[8] == 0 {
            ctx.input[9] = plus_one(ctx.input[9]);
            // Note: Stopping at 2^70 bytes per nonce is user's responsibility
        }

        if bytes <= 64 {
            for i in 0..bytes {
                c[c_offset + i] = m[m_offset + i] ^ output[i];
            }
            return;
        }

        for i in 0..64 {
            c[c_offset + i] = m[m_offset + i] ^ output[i];
        }
        bytes -= 64;
        m_offset += 64;
        c_offset += 64;
    }
}

pub fn salsa20_decrypt_bytes(ctx: &mut Salsa20Context, c: &[u8], m: &mut [u8]) {
    // Salsa20 decryption is identical to encryption due to XOR
    salsa20_encrypt_bytes(ctx, c, m);
}
