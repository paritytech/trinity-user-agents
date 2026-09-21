// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use async_trait::async_trait;
use parity_scale_codec::{Decode, Encode};

const CLAIM_PLAN_DATA_V1_MAGIC: &[u8; 4] = b"CPV1";

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
    pub claimed_amount: Option<u128>,
    pub total_value: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimPlanDataV1 {
    entries: Vec<CodableClaimPlanEntry>,
    outgoing_public_keys: Vec<[u8; 32]>,
    detection_anchor: Option<[u8; 32]>,
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

/// Versioned payload stored in the legacy `entries_data` column. Old rows
/// contained only `Vec<CodableClaimPlanEntry>` and remain readable; new rows
/// append the outgoing monitor's public-only evidence without a schema
/// migration or secret-bearing data.
pub fn encode_claim_plan_data(plan: &ClaimPlan) -> Vec<u8> {
    let mut encoded = CLAIM_PLAN_DATA_V1_MAGIC.to_vec();
    encoded.extend(
        ClaimPlanDataV1 {
            entries: plan.entries.clone(),
            outgoing_public_keys: plan.outgoing_public_keys.clone(),
            detection_anchor: plan.detection_anchor,
        }
        .encode(),
    );
    encoded
}

/// Decoded `claim_plans.entries_data` payload: the plan entries, the
/// outgoing public keys, and the optional detection anchor (both absent on
/// pre-v1 blobs).
pub type DecodedClaimPlanData = (Vec<CodableClaimPlanEntry>, Vec<[u8; 32]>, Option<[u8; 32]>);

pub fn decode_claim_plan_data(bytes: &[u8]) -> Result<DecodedClaimPlanData, String> {
    let Some(payload) = bytes.strip_prefix(CLAIM_PLAN_DATA_V1_MAGIC) else {
        return decode_claim_plan_entries(bytes).map(|entries| (entries, Vec::new(), None));
    };
    let mut input = payload;
    let decoded = ClaimPlanDataV1::decode(&mut input)
        .map_err(|error| format!("claim plan data v1: {error}"))?;
    if !input.is_empty() {
        return Err("claim plan data v1: trailing bytes".into());
    }
    Ok((
        decoded.entries,
        decoded.outgoing_public_keys,
        decoded.detection_anchor,
    ))
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
    fn versioned_plan_data_round_trips_and_legacy_rows_remain_readable() {
        let plan = ClaimPlan {
            memo_key: [1; 32],
            message_id: Some("message".into()),
            entries: vec![CodableClaimPlanEntry {
                entry_index: 0,
                exponent: 3,
                derivation_index: 7,
            }],
            outgoing_public_keys: vec![[8; 32], [9; 32]],
            detection_anchor: Some([10; 32]),
            status: ClaimPlanStatus::Processing,
            claimed_amount: None,
            total_value: 80,
        };
        assert_eq!(
            decode_claim_plan_data(&encode_claim_plan_data(&plan)).unwrap(),
            (
                plan.entries.clone(),
                plan.outgoing_public_keys.clone(),
                plan.detection_anchor
            )
        );

        let legacy = encode_claim_plan_entries(&plan.entries);
        assert_eq!(
            decode_claim_plan_data(&legacy).unwrap(),
            (plan.entries, Vec::new(), None)
        );
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
