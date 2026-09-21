// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use blake2::Blake2b;
use blake2::digest::Digest;
use blake2::digest::consts::U32;
use parity_scale_codec::{Compact, Decode, Encode, Input};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// One raw 64-byte coin secret key.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct MemoEntry(pub [u8; 64]);

impl std::fmt::Debug for MemoEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoEntry(..)")
    }
}

/// The transfer memo: entries plus the expected total value in planks.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct TransferMemo {
    pub entries: Vec<MemoEntry>,
    pub total_value: u128,
}

impl std::fmt::Debug for TransferMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TransferMemo {{ entries: {}, .. }}", self.entries.len())
    }
}

impl TransferMemo {
    /// Exact iOS `TransferMemo.encode(scaleEncoder:)` layout: SCALE
    /// `Vec<Vec<u8>>` entries followed by a SCALE compact `BigUInt`
    /// total. The returned bytes are secret material: callers must retain
    /// them only in zeroizing buffers and must never log or expose them.
    pub fn scale_encoded(&self) -> Vec<u8> {
        let count = u32::try_from(self.entries.len()).expect("memo entry count fits SCALE u32");
        let mut encoded =
            Vec::with_capacity(self.entries.len().saturating_mul(66).saturating_add(22));
        Compact(count).encode_to(&mut encoded);
        for entry in &self.entries {
            Compact(64u32).encode_to(&mut encoded);
            encoded.extend_from_slice(&entry.0);
        }
        Compact(self.total_value).encode_to(&mut encoded);
        encoded
    }

    /// Strict inverse of [`Self::scale_encoded`]. Every entry must be one
    /// 64-byte expanded sr25519 secret, and trailing bytes reject.
    /// Partially decoded keys are wiped on every exit.
    pub fn from_scale_encoded(bytes: &[u8]) -> Result<Self, String> {
        let mut input = bytes;
        let count = Compact::<u32>::decode(&mut input)
            .map_err(|error| format!("transfer memo entry count decode failed: {error}"))?
            .0 as usize;
        if count > input.len() / 66 {
            return Err("transfer memo entries exceed its encoded length".into());
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let length = Compact::<u32>::decode(&mut input)
                .map_err(|error| format!("transfer memo entry length decode failed: {error}"))?
                .0;
            if length != 64 {
                return Err(format!(
                    "transfer memo entry is {length} bytes; expected 64"
                ));
            }
            let mut entry = MemoEntry([0; 64]);
            input
                .read(&mut entry.0)
                .map_err(|error| format!("transfer memo entry decode failed: {error}"))?;
            entries.push(entry);
        }
        let total_value = Compact::<u128>::decode(&mut input)
            .map_err(|error| format!("transfer memo total decode failed: {error}"))?
            .0;
        if !input.is_empty() {
            return Err("transfer memo has trailing bytes".into());
        }
        Ok(Self {
            entries,
            total_value,
        })
    }

    pub fn identifier(&self) -> [u8; 32] {
        let value_be = self.total_value.to_be_bytes();
        let first_nonzero = value_be
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(value_be.len());
        let value = &value_be[first_nonzero..];
        let Some((first, rest)) = self.entries.split_first() else {
            return blake2b_256(value);
        };
        let mut acc = blake2b_256_keyed(&first.0, value);
        for entry in rest {
            acc = blake2b_256_keyed(&entry.0, &acc);
        }
        acc
    }
}

fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn blake2b_256_keyed(key: &[u8], data: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .key(key)
        .hash(data);
    <[u8; 32]>::try_from(hash.as_bytes()).expect("hash_length is 32")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memo() -> TransferMemo {
        TransferMemo {
            entries: vec![MemoEntry([1u8; 64]), MemoEntry([2u8; 64])],
            total_value: 1_234_567,
        }
    }

    #[test]
    fn identifier_is_deterministic() {
        assert_eq!(memo().identifier(), memo().identifier());
    }

    #[test]
    fn identifier_matches_the_reference_fold_layout() {
        let memo = TransferMemo {
            entries: vec![MemoEntry([0x11; 64]), MemoEntry([0x22; 64])],
            total_value: 100,
        };
        // BigUInt(100).serialize == [0x64] — one byte, not 16.
        let step1 = blake2b_simd::Params::new()
            .hash_length(32)
            .key(&[0x11; 64])
            .hash(&[0x64]);
        let step2 = blake2b_simd::Params::new()
            .hash_length(32)
            .key(&[0x22; 64])
            .hash(step1.as_bytes());
        assert_eq!(
            memo.identifier(),
            <[u8; 32]>::try_from(step2.as_bytes()).unwrap()
        );

        // BigUInt(0).serialize is EMPTY — the fold starts from zero bytes.
        let zero = TransferMemo {
            entries: vec![MemoEntry([0x11; 64])],
            total_value: 0,
        };
        let expected = blake2b_simd::Params::new()
            .hash_length(32)
            .key(&[0x11; 64])
            .hash(&[]);
        assert_eq!(
            zero.identifier(),
            <[u8; 32]>::try_from(expected.as_bytes()).unwrap()
        );
    }

    #[test]
    fn scale_layout_matches_ios_vec_data_then_compact_biguint() {
        let memo = TransferMemo {
            entries: vec![MemoEntry([0xAB; 64])],
            total_value: 1_000,
        };
        let encoded = memo.scale_encoded();
        assert_eq!(encoded[0], 0x04, "one-entry Vec compact prefix");
        assert_eq!(&encoded[1..3], &[0x01, 0x01], "64-byte Data prefix");
        assert_eq!(&encoded[3..67], &[0xAB; 64]);
        assert_eq!(&encoded[67..], &[0xA1, 0x0F], "compact 1000");

        let decoded = TransferMemo::from_scale_encoded(&encoded).unwrap();
        assert_eq!(decoded.entries, memo.entries);
        assert_eq!(decoded.total_value, memo.total_value);
    }

    #[test]
    fn scale_decode_rejects_wrong_key_lengths_and_trailing_bytes() {
        let mut wrong = vec![vec![1u8; 63]].encode();
        Compact(1u128).encode_to(&mut wrong);
        assert!(TransferMemo::from_scale_encoded(&wrong).is_err());

        let mut trailing = memo().scale_encoded();
        trailing.push(0);
        assert!(TransferMemo::from_scale_encoded(&trailing).is_err());
    }

    #[test]
    fn identifier_binds_value_entries_and_order() {
        let base = memo().identifier();
        let mut other = memo();
        other.total_value += 1;
        assert_ne!(base, other.identifier(), "total value is bound");

        let mut reordered = memo();
        reordered.entries.reverse();
        assert_ne!(base, reordered.identifier(), "entry order is bound");

        let mut truncated = memo();
        truncated.entries.pop();
        assert_ne!(base, truncated.identifier(), "every entry is bound");
    }

    #[test]
    fn zeroize_clears_entry_bytes() {
        let mut entry = MemoEntry([0xAB; 64]);
        entry.zeroize();
        assert_eq!(entry.0, [0u8; 64]);
    }

    #[test]
    fn zeroize_clears_the_whole_memo() {
        let mut memo = memo();
        memo.zeroize();
        assert!(memo.entries.is_empty(), "entries are dropped and wiped");
        assert_eq!(memo.total_value, 0);
    }

    #[test]
    fn debug_never_prints_key_bytes() {
        let rendered = format!("{:?} {:?}", memo(), MemoEntry([0xCD; 64]));
        assert!(!rendered.contains("205"), "no decimal byte dump");
        assert!(!rendered.to_lowercase().contains("cd"), "no hex byte dump");
        assert_eq!(rendered, "TransferMemo { entries: 2, .. } MemoEntry(..)");
    }
}
