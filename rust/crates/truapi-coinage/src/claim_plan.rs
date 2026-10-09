// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use async_trait::async_trait;
use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CodableClaimPlanEntry {
    pub entry_index: i16,
    /// The destination coin's denomination exponent.
    pub exponent: i16,
    /// The destination coin's derivation index.
    pub derivation_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimPlanStatus {
    Processing,
    /// Coins confirmed on-chain (outgoing: awaiting recipient claim;
    /// incoming: ready to submit the claim extrinsic).
    Detected,
    Finished,
    Error,
}

impl ClaimPlanStatus {
    pub fn as_raw(self) -> i64 {
        match self {
            ClaimPlanStatus::Processing => 0,
            ClaimPlanStatus::Detected => 1,
            ClaimPlanStatus::Finished => 2,
            ClaimPlanStatus::Error => 3,
        }
    }

    pub fn from_raw(raw: i64) -> Option<Self> {
        Some(match raw {
            0 => ClaimPlanStatus::Processing,
            1 => ClaimPlanStatus::Detected,
            2 => ClaimPlanStatus::Finished,
            3 => ClaimPlanStatus::Error,
            _ => return None,
        })
    }
}

/// Durable per-entry progress for a claim of externally supplied secrets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimMarkers {
    /// Entries whose transfer this wallet submitted, recorded before submission.
    pub submitted: Vec<i16>,
    /// Entries absent at finalized state that this wallet never submitted:
    /// their source was spent elsewhere, so they can never be credited.
    pub forfeited: Vec<i16>,
    /// Total value of the forfeited entries.
    pub forfeited_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimPlan {
    pub memo_key: [u8; 32],
    /// The chat message carrying the memo, when known.
    pub message_id: Option<String>,
    pub entries: Vec<CodableClaimPlanEntry>,
    pub outgoing_public_keys: Vec<[u8; 32]>,
    /// Best-effort finalized snapshot captured before durable chat
    /// acceptance. Exact/pass-through coins may already be visible here,
    /// which provides historical evidence for an ultra-fast recipient claim.
    pub detection_anchor: Option<[u8; 32]>,
    pub status: ClaimPlanStatus,
    /// Value of the processed entry prefix, including forfeited entries.
    pub claimed_amount: Option<u128>,
    pub total_value: u128,
    pub markers: ClaimMarkers,
}

impl ClaimPlan {
    /// Value actually credited to this wallet: the processed prefix without
    /// forfeited entries. `None` when nothing was processed or the markers
    /// exceed the prefix.
    pub fn credited_amount(&self) -> Option<u128> {
        self.claimed_amount?
            .checked_sub(self.markers.forfeited_value)
    }
}

/// SCALE-encode the entries blob for `claim_plans.entries_data`.
pub fn encode_claim_plan_entries(entries: &[CodableClaimPlanEntry]) -> Vec<u8> {
    entries.encode()
}

/// Decode an `entries_data` blob; errors on malformed or trailing bytes.
pub fn decode_claim_plan_entries(bytes: &[u8]) -> Result<Vec<CodableClaimPlanEntry>, String> {
    let mut input = bytes;
    let entries = Vec::<CodableClaimPlanEntry>::decode(&mut input)
        .map_err(|error| format!("claim plan entries: {error}"))?;
    if !input.is_empty() {
        return Err("claim plan entries: trailing bytes".into());
    }
    Ok(entries)
}

#[async_trait]
pub trait ClaimPlanStore: Send + Sync {
    /// Full save (insert or replace, re-encoding entries).
    async fn save(&self, plan: &ClaimPlan) -> Result<(), String>;

    /// Lookup by memo key.
    async fn plan(&self, memo_key: &[u8; 32]) -> Result<Option<ClaimPlan>, String>;

    async fn load_all(&self) -> Result<Vec<ClaimPlan>, String>;

    async fn update_status(
        &self,
        memo_key: &[u8; 32],
        status: ClaimPlanStatus,
        claimed_amount: Option<u128>,
    ) -> Result<(), String>;

    async fn remove(&self, memo_key: &[u8; 32]) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_scale_layout_is_pinned() {
        let entry = CodableClaimPlanEntry {
            entry_index: 1,
            exponent: -2,
            derivation_index: 0x0403_0201,
        };
        // i16 LE ++ i16 LE ++ u32 LE = 8 bytes, fixed.
        assert_eq!(entry.encode(), vec![1, 0, 0xFE, 0xFF, 1, 2, 3, 4]);
    }

    #[test]
    fn entries_blob_round_trips() {
        let entries = vec![
            CodableClaimPlanEntry {
                entry_index: 0,
                exponent: 3,
                derivation_index: 7,
            },
            CodableClaimPlanEntry {
                entry_index: 1,
                exponent: -1,
                derivation_index: 8,
            },
        ];
        let blob = encode_claim_plan_entries(&entries);
        assert_eq!(decode_claim_plan_entries(&blob).unwrap(), entries);
        assert!(
            decode_claim_plan_entries(&[]).is_err(),
            "empty blob is malformed"
        );
        let mut trailing = blob.clone();
        trailing.push(0);
        assert!(decode_claim_plan_entries(&trailing).is_err());
    }

    #[test]
    fn status_raw_round_trips() {
        for status in [
            ClaimPlanStatus::Processing,
            ClaimPlanStatus::Detected,
            ClaimPlanStatus::Finished,
            ClaimPlanStatus::Error,
        ] {
            assert_eq!(ClaimPlanStatus::from_raw(status.as_raw()), Some(status));
        }
        assert_eq!(ClaimPlanStatus::from_raw(4), None);
    }
}
