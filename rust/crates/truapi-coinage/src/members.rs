// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-chain/src/pallets/members.rs.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! SCALE layouts used by Coinage recycler queries.
use parity_scale_codec::{Decode, Encode};

/// A ring collection's 32-byte identifier.
pub type CollectionIdentifier = [u8; 32];

/// A member's 32-byte ring-VRF (bandersnatch) public key.
pub type MemberKey = [u8; 32];

pub type RingIndex = u32;

/// Where a member key currently sits within a collection
/// (`indiv_support::traits::reality::RingPosition`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum RingPosition {
    #[codec(index = 0)]
    Onboarding { queue_page: u32, queued_at: u64 },
    #[codec(index = 1)]
    Included {
        ring_index: u32,
        ring_page: u32,
        ring_position: u32,
    },
    #[codec(index = 2)]
    Suspended,
}

/// `Members.RingKeysStatus` value (`indiv_support::…::RingStatus`). It
/// stores `total` and `included` (queued = `total - included`) plus the
/// timestamp the ring became immutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct RingStatus {
    pub total: u32,
    pub included: u32,
    /// Seconds timestamp once the ring stopped accepting keys.
    pub immutable_since: Option<u64>,
}

impl RingStatus {
    /// Keys queued behind the proof set.
    pub fn queued(&self) -> u32 {
        self.total.saturating_sub(self.included)
    }
}

/// `Members.Root` value: the current ring root commitment.
///
/// 2026-08 wipe (spec 1000032): the root commitment shrank from 768 to
/// 288 bytes (live value = 288 + 4 + 848 = 1140 bytes, probed via
/// `brevity-ffi/examples/ring_root_type_probe.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingRoot {
    pub root: [u8; 288],
    pub revision: u32,
    pub intermediate: [u8; 848],
}
