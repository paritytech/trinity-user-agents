// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use async_trait::async_trait;
use parity_scale_codec::{Decode, Encode, Error as CodecError, Input};

use crate::constants::WAL_MORTALITY_BLOCKS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalOperation {
    /// Voucher unload into coins — recovery probes the expected output
    /// coins on-chain.
    IntoCoins,
    /// Voucher offboard into an external asset — recovery checks the
    /// input vouchers were consumed.
    IntoExternalAsset,
    /// Coin recycling — recovery checks the input coin was consumed and
    /// the surplus voucher appeared.
    RecycleIntoVoucher,
    /// Whole coin secrets were (or may have been) handed off out of band.
    /// Unlike an extrinsic, a memo has no mortal era. Recovery therefore
    /// retains its input reservation while any input is still on-chain and
    /// retires it only once every input is absent. The live sender deletes
    /// this entry only when a handoff reports an explicit pre-acceptance
    /// rejection.
    SecretHandoff,
    /// A regular Coinage split. It has coin inputs and expected coin
    /// outputs like `IntoCoins`, but remains distinct so recovery and
    /// diagnostics never infer that vouchers were involved.
    Split,
    /// Durable operation receipt. Prepared does not prove whether the
    /// idempotent transport accepted the memo before a process stopped.
    TransferPrepared,
    TransferAccepted,
    /// All outgoing allocations finalized successfully or were observed with
    /// exact outputs and consumed inputs during recovery. Pass-through inputs
    /// were retired locally. This does not itself prove recipient claim.
    TransferCompleted,
    /// Transport definitively rejected the memo before accepting it.
    TransferRejected,
}

impl WalOperation {
    pub fn as_raw(self) -> i64 {
        match self {
            WalOperation::IntoCoins => 0,
            WalOperation::IntoExternalAsset => 1,
            WalOperation::RecycleIntoVoucher => 2,
            WalOperation::SecretHandoff => 3,
            WalOperation::Split => 4,
            WalOperation::TransferPrepared => 5,
            WalOperation::TransferAccepted => 6,
            WalOperation::TransferCompleted => 7,
            WalOperation::TransferRejected => 8,
        }
    }

    pub fn from_raw(raw: i64) -> Option<Self> {
        Some(match raw {
            0 => WalOperation::IntoCoins,
            1 => WalOperation::IntoExternalAsset,
            2 => WalOperation::RecycleIntoVoucher,
            3 => WalOperation::SecretHandoff,
            4 => WalOperation::Split,
            5 => WalOperation::TransferPrepared,
            6 => WalOperation::TransferAccepted,
            7 => WalOperation::TransferCompleted,
            8 => WalOperation::TransferRejected,
            _ => return None,
        })
    }

    pub fn is_transfer_receipt(self) -> bool {
        matches!(
            self,
            Self::TransferPrepared
                | Self::TransferAccepted
                | Self::TransferCompleted
                | Self::TransferRejected
        )
    }
}

/// One derived asset referenced from a WAL payload: enough to re-derive
/// its key (index) and materialize it locally (exponent) at recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct WalCoinRef {
    pub derivation_index: u32,
    pub exponent: i16,
}

/// The SCALE payload of a WAL entry (`transfer_wal_entries.payload`):
/// inputs consumed and outputs expected by the journaled extrinsic.
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode)]
pub struct WalPayload {
    pub input_coins: Vec<WalCoinRef>,
    pub input_vouchers: Vec<WalCoinRef>,
    pub output_coins: Vec<WalCoinRef>,
    pub output_vouchers: Vec<WalCoinRef>,
    /// Subset of `output_coins` delivered in the recipient memo. Recovery
    /// materializes these as locally `Spent`, while change remains
    /// `Available`. Appending the field preserves the v1 prefix layout.
    pub destination_coins: Vec<WalCoinRef>,
}

impl Decode for WalPayload {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let input_coins = Vec::<WalCoinRef>::decode(input)?;
        let input_vouchers = Vec::<WalCoinRef>::decode(input)?;
        let output_coins = Vec::<WalCoinRef>::decode(input)?;
        let output_vouchers = Vec::<WalCoinRef>::decode(input)?;
        // Rows written before destination/change distinction end after the
        // fourth vector. Treat that exact legacy shape as all-change.
        let destination_coins = match input.remaining_len()? {
            Some(0) => Vec::new(),
            _ => Vec::<WalCoinRef>::decode(input)?,
        };
        Ok(Self {
            input_coins,
            input_vouchers,
            output_coins,
            output_vouchers,
            destination_coins,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointBlock {
    Pending,
    Known { number: u64, hash: [u8; 32] },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferWalEntry {
    pub entry_id: String,
    pub operation: WalOperation,
    pub payload: WalPayload,
    pub checkpoint: CheckpointBlock,
    pub created_at_ms: i64,
}

/// Hex encoding makes the operation/child boundary unambiguous even when a
/// host identifier contains punctuation. All children share this namespace.
pub fn operation_entry_id(operation_id: &str, child: &str) -> String {
    format!(
        "cash-operation:{}:{child}",
        hex::encode(operation_id.as_bytes())
    )
}

impl TransferWalEntry {
    pub fn belongs_to_operation(&self, operation_id: &str) -> bool {
        self.entry_id
            .starts_with(&operation_entry_id(operation_id, ""))
    }

    /// The durable receipt identifier for any correlated child or receipt.
    pub fn operation_parent_id(&self) -> Option<String> {
        let suffix = self.entry_id.strip_prefix("cash-operation:")?;
        let (operation, child) = suffix.split_once(':')?;
        if operation.is_empty() || child.is_empty() {
            return None;
        }
        Some(format!("cash-operation:{operation}:parent"))
    }

    pub fn is_expired(&self, finalized_block: u64) -> bool {
        match self.checkpoint {
            CheckpointBlock::Pending => true,
            CheckpointBlock::Known { number, .. } => {
                finalized_block > number.saturating_add(WAL_MORTALITY_BLOCKS)
            }
        }
    }

    pub fn is_forked(&self, canonical_hash_at_checkpoint: Option<&[u8; 32]>) -> bool {
        match (&self.checkpoint, canonical_hash_at_checkpoint) {
            (CheckpointBlock::Known { hash, .. }, Some(canonical)) => hash != canonical,
            _ => false,
        }
    }
}

/// Durable WAL persistence. `save_all` must commit the whole batch atomically;
/// checkpoint writes must be durable before an adapter broadcasts.
#[async_trait]
pub trait WalStore: Send + Sync {
    /// Inserts or durably replaces the entry with the same identifier.
    async fn save(&self, entry: &TransferWalEntry) -> Result<(), String>;

    async fn save_all(&self, entries: &[TransferWalEntry]) -> Result<(), String>;

    async fn update_checkpoint(
        &self,
        entry_id: &str,
        checkpoint: CheckpointBlock,
    ) -> Result<(), String>;

    async fn load_all(&self) -> Result<Vec<TransferWalEntry>, String>;

    /// Includes the permanent parent receipt even after every child settles.
    async fn load_operation(&self, operation_id: &str) -> Result<Vec<TransferWalEntry>, String> {
        let prefix = operation_entry_id(operation_id, "");
        Ok(self
            .load_all()
            .await?
            .into_iter()
            .filter(|entry| entry.entry_id.starts_with(&prefix))
            .collect())
    }

    async fn delete(&self, entry_id: &str) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(checkpoint: CheckpointBlock) -> TransferWalEntry {
        TransferWalEntry {
            entry_id: "e1".into(),
            operation: WalOperation::IntoCoins,
            payload: WalPayload::default(),
            checkpoint,
            created_at_ms: 0,
        }
    }

    #[test]
    fn mortality_boundary_is_checkpoint_plus_300() {
        let known = entry(CheckpointBlock::Known {
            number: 1_000,
            hash: [1; 32],
        });
        assert!(!known.is_expired(1_300), "at the boundary: still alive");
        assert!(known.is_expired(1_301), "one past the boundary: dead");
        let near_max = entry(CheckpointBlock::Known {
            number: u64::MAX - 10,
            hash: [1; 32],
        });
        assert!(!near_max.is_expired(u64::MAX), "saturating add, no wrap");
    }

    #[test]
    fn pending_checkpoint_is_immediately_expired() {
        assert!(entry(CheckpointBlock::Pending).is_expired(0));
    }

    #[test]
    fn fork_detection_compares_canonical_hash() {
        let known = entry(CheckpointBlock::Known {
            number: 5,
            hash: [1; 32],
        });
        assert!(known.is_forked(Some(&[2; 32])));
        assert!(!known.is_forked(Some(&[1; 32])));
        assert!(!known.is_forked(None), "unavailable hash is not a fork");
        assert!(!entry(CheckpointBlock::Pending).is_forked(Some(&[2; 32])));
    }

    #[test]
    fn operation_raw_round_trips() {
        for op in [
            WalOperation::IntoCoins,
            WalOperation::IntoExternalAsset,
            WalOperation::RecycleIntoVoucher,
            WalOperation::SecretHandoff,
            WalOperation::Split,
            WalOperation::TransferPrepared,
            WalOperation::TransferAccepted,
            WalOperation::TransferCompleted,
            WalOperation::TransferRejected,
        ] {
            assert_eq!(WalOperation::from_raw(op.as_raw()), Some(op));
        }
        assert_eq!(WalOperation::from_raw(9), None);
    }

    #[test]
    fn payload_scale_round_trips() {
        let payload = WalPayload {
            input_coins: vec![WalCoinRef {
                derivation_index: 1,
                exponent: 4,
            }],
            input_vouchers: vec![],
            output_coins: vec![
                WalCoinRef {
                    derivation_index: 9,
                    exponent: 2,
                },
                WalCoinRef {
                    derivation_index: 10,
                    exponent: -1,
                },
            ],
            output_vouchers: vec![],
            destination_coins: vec![WalCoinRef {
                derivation_index: 9,
                exponent: 2,
            }],
        };
        let encoded = payload.encode();
        assert_eq!(WalPayload::decode(&mut &encoded[..]).unwrap(), payload);
    }

    #[test]
    fn legacy_payload_without_destination_suffix_still_decodes() {
        let legacy = (
            vec![WalCoinRef {
                derivation_index: 1,
                exponent: 0,
            }],
            Vec::<WalCoinRef>::new(),
            vec![WalCoinRef {
                derivation_index: 2,
                exponent: 1,
            }],
            Vec::<WalCoinRef>::new(),
        )
            .encode();
        let decoded = WalPayload::decode(&mut &legacy[..]).unwrap();
        assert!(decoded.destination_coins.is_empty());
        assert_eq!(decoded.output_coins[0].derivation_index, 2);
    }
    #[test]
    fn operation_namespace_cannot_match_a_different_host_identifier() {
        let mut child = entry(CheckpointBlock::Pending);
        child.entry_id = operation_entry_id("wallet:payment", "unload-0");
        assert!(child.belongs_to_operation("wallet:payment"));
        assert!(!child.belongs_to_operation("wallet"));
        assert!(!child.belongs_to_operation("wallet:payment:unload"));
        assert_eq!(
            child.operation_parent_id(),
            Some(operation_entry_id("wallet:payment", "parent"))
        );
        child.entry_id = "legacy-split".into();
        assert_eq!(child.operation_parent_id(), None);
    }
}
