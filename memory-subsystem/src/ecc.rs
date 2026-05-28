//! # Hamming SECDED (13,8) Error-Correcting Code Module
//!
//! This module implements Single Error Correction, Double Error Detection (SECDED)
//! using Hamming (12,8) code combined with an overall parity bit (making it 13 bits total,
//! represented inside a `u16` word).
//!
//! Memory pages inside the `memory-subsystem` can utilize this encoding to identify
//! and recover from silent data corruption (bit flips) caused by simulated cosmic rays or hardware faults.
//!
//! ## Theory of SECDED
//! - **Single-bit flip**: Causes the overall parity check to fail (odd parity parity check discrepancy)
//!   and the syndrome $S \neq 0$ points directly to the flipped bit position (1-indexed).
//! - **Double-bit flip**: Overall parity check passes (even parity preserved) but the syndrome $S \neq 0$,
//!   indicating an uncorrectable multi-bit error.
//! - **No error**: Overall parity check passes and syndrome $S = 0$.

/// The result of attempting to decode a SECDED-encoded 13-bit word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeResult {
    /// The word is perfectly intact with no errors.
    NoError(u8),
    /// A single-bit flip was detected and successfully corrected.
    /// Contains the corrected byte and the 1-indexed bit position that was flipped (1..=13).
    Corrected(u8, usize),
    /// A double-bit flip was detected. This error is uncorrectable.
    Uncorrectable,
}

/// Encodes an 8-bit byte into a 13-bit SECDED word stored in a `u16`.
///
/// The layout of the 13-bit word is as follows:
/// - Bit 1 (0x01): Parity Bit $p_1$
/// - Bit 2 (0x02): Parity Bit $p_2$
/// - Bit 3 (0x04): Data Bit $d_0$
/// - Bit 4 (0x08): Parity Bit $p_3$
/// - Bit 5 (0x10): Data Bit $d_1$
/// - Bit 6 (0x20): Data Bit $d_2$
/// - Bit 7 (0x40): Data Bit $d_3$
/// - Bit 8 (0x80): Parity Bit $p_4$
/// - Bit 9 (0x100): Data Bit $d_4$
/// - Bit 10 (0x200): Data Bit $d_5$
/// - Bit 11 (0x400): Data Bit $d_6$
/// - Bit 12 (0x800): Data Bit $d_7$
/// - Bit 13 (0x1000): Overall Parity Bit $p_5$ (covers bits 1..12)
///
/// ### Example
/// ```
/// use memory_subsystem::ecc::{encode, decode, DecodeResult};
/// let encoded = encode(0x55);
/// assert!(matches!(decode(encoded), DecodeResult::NoError(0x55)));
/// ```
pub fn encode(data: u8) -> u16 {
    let d0 = (data & 1) as u16;
    let d1 = ((data >> 1) & 1) as u16;
    let d2 = ((data >> 2) & 1) as u16;
    let d3 = ((data >> 3) & 1) as u16;
    let d4 = ((data >> 4) & 1) as u16;
    let d5 = ((data >> 5) & 1) as u16;
    let d6 = ((data >> 6) & 1) as u16;
    let d7 = ((data >> 7) & 1) as u16;

    // Compute Hamming parity bits
    let p1 = d0 ^ d1 ^ d3 ^ d4 ^ d6;
    let p2 = d0 ^ d2 ^ d3 ^ d5 ^ d6;
    let p3 = d1 ^ d2 ^ d3 ^ d7;
    let p4 = d4 ^ d5 ^ d6 ^ d7;

    // Map bits into their 12-bit positions (0-indexed for shift operations, so 1-indexed positions become shift by position-1)
    let mut word = p1 
        | (p2 << 1) 
        | (d0 << 2) 
        | (p3 << 3) 
        | (d1 << 4) 
        | (d2 << 5) 
        | (d3 << 6) 
        | (p4 << 7) 
        | (d4 << 8) 
        | (d5 << 9) 
        | (d6 << 10) 
        | (d7 << 11);

    // Compute overall parity bit (p5 at position 13) covering bits 1..12
    let p5 = (word.count_ones() as u16) & 1;
    word |= p5 << 12;

    word
}

/// Decodes a 13-bit SECDED word, identifying and fixing single-bit errors or flagging double-bit errors.
///
/// ### Error Detection & Correction Logic
/// 1. Extract the bits and compute the 4-bit syndrome $S$.
/// 2. Compute the overall parity of the 13 bits.
/// 3. If the overall parity is incorrect (odd number of set bits), we have a single error:
///    - If $S \neq 0$, the flipped bit is at 1-indexed position $S$. We correct it.
///    - If $S = 0$, the overall parity bit itself was flipped. The data is correct.
/// 4. If the overall parity is correct (even number of set bits):
///    - If $S = 0$, there are no errors.
///    - If $S \neq 0$, a double-bit error occurred (uncorrectable).
pub fn decode(word: u16) -> DecodeResult {
    let b1 = word & 1;
    let b2 = (word >> 1) & 1;
    let b3 = (word >> 2) & 1;
    let b4 = (word >> 3) & 1;
    let b5 = (word >> 4) & 1;
    let b6 = (word >> 5) & 1;
    let b7 = (word >> 6) & 1;
    let b8 = (word >> 7) & 1;
    let b9 = (word >> 8) & 1;
    let b10 = (word >> 9) & 1;
    let b11 = (word >> 10) & 1;
    let b12 = (word >> 11) & 1;
    let _b13 = (word >> 12) & 1;

    // Calculate syndrome bits
    let s1 = b1 ^ b3 ^ b5 ^ b7 ^ b9 ^ b11;
    let s2 = b2 ^ b3 ^ b6 ^ b7 ^ b10 ^ b11;
    let s3 = b4 ^ b5 ^ b6 ^ b7 ^ b12;
    let s4 = b8 ^ b9 ^ b10 ^ b11 ^ b12;

    let syndrome = s1 | (s2 << 1) | (s3 << 2) | (s4 << 3);

    // Parity of the entire 13-bit word. 
    // In even parity, the sum of all bits should be 0. An odd sum means a parity violation.
    let total_parity_violation = (word.count_ones() & 1) != 0;

    if total_parity_violation {
        // Single-bit error detected
        if syndrome == 0 {
            // The parity bit itself (bit 13) is corrupted, data is intact!
            let data = extract_data(word);
            DecodeResult::Corrected(data, 13)
        } else if syndrome <= 12 {
            // Correct the flipped bit in the word
            let corrected_word = word ^ (1 << (syndrome - 1));
            let data = extract_data(corrected_word);
            DecodeResult::Corrected(data, syndrome as usize)
        } else {
            // Invalid syndrome mapping, treat as uncorrectable
            DecodeResult::Uncorrectable
        }
    } else {
        // No parity violation: either 0 errors or double error
        if syndrome == 0 {
            DecodeResult::NoError(extract_data(word))
        } else {
            // Double-bit error detected (parity check passed but syndrome is non-zero)
            DecodeResult::Uncorrectable
        }
    }
}

/// Helper function to extract data bits from a valid 12-bit Hamming word.
fn extract_data(word: u16) -> u8 {
    let d0 = (word >> 2) & 1;
    let d1 = (word >> 4) & 1;
    let d2 = (word >> 5) & 1;
    let d3 = (word >> 6) & 1;
    let d4 = (word >> 8) & 1;
    let d5 = (word >> 9) & 1;
    let d6 = (word >> 10) & 1;
    let d7 = (word >> 11) & 1;

    (d0 | (d1 << 1) | (d2 << 2) | (d3 << 3) | (d4 << 4) | (d5 << 5) | (d6 << 6) | (d7 << 7)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_error() {
        for val in 0..=255 {
            let encoded = encode(val);
            assert_eq!(decode(encoded), DecodeResult::NoError(val));
        }
    }

    #[test]
    fn test_single_bit_flip_correction() {
        let val = 0xA5; // 10100101
        let encoded = encode(val);

        // Flip each of the 13 bits and verify recovery
        for i in 0..13 {
            let corrupted = encoded ^ (1 << i);
            match decode(corrupted) {
                DecodeResult::Corrected(decoded, flipped_bit) => {
                    assert_eq!(decoded, val, "Correction failed for bit {}", i + 1);
                    assert_eq!(flipped_bit, i + 1, "Incorrect bit position reported");
                }
                other => panic!("Expected Corrected, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_double_bit_flip_detection() {
        let val = 0x3C;
        let encoded = encode(val);

        // Flip pairs of bits and verify that it flags as Uncorrectable
        for i in 0..13 {
            for j in (i + 1)..13 {
                let corrupted = encoded ^ (1 << i) ^ (1 << j);
                assert_eq!(
                    decode(corrupted),
                    DecodeResult::Uncorrectable,
                    "Failed to detect double bit flip at {}, {}",
                    i + 1,
                    j + 1
                );
            }
        }
    }
}
