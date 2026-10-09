// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded SCALE snapshots. Plaintext staging never leaves a zeroizing buffer.

use parity_scale_codec::{Decode, DecodeLimit, Encode, MemTrackingInput, Output};
use zeroize::Zeroizing;

use super::{ChatError, MAX_DECODE_BYTES, MAX_DECODE_DEPTH, MAX_SNAPSHOT_BYTES, TAG_BYTES};

struct Size(usize);

impl Output for Size {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = self.0.saturating_add(bytes.len());
    }
}

struct BoundedOutput {
    bytes: Zeroizing<Vec<u8>>,
    limit: usize,
    overflow: bool,
}

impl Output for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) {
        if self.overflow || bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.overflow = true;
        } else {
            self.bytes.extend_from_slice(bytes);
        }
    }
}

pub(super) fn encode<T: Encode>(state: &T) -> Result<Zeroizing<Vec<u8>>, ChatError> {
    // Do not trust size_hint(), nor allow Vec growth to abandon plaintext in
    // a freed allocation. The second pass is bounded even for a custom Encode.
    let mut size = Size(0);
    state.encode_to(&mut size);
    if size.0 > MAX_SNAPSHOT_BYTES {
        return Err(ChatError::StorageUnavailable);
    }
    let mut output = BoundedOutput {
        bytes: Zeroizing::new(Vec::with_capacity(size.0 + TAG_BYTES)),
        limit: size.0,
        overflow: false,
    };
    state.encode_to(&mut output);
    if output.overflow || output.bytes.len() != size.0 {
        return Err(ChatError::StorageUnavailable);
    }
    Ok(output.bytes)
}

pub(super) fn decode<T: Decode>(plaintext: &[u8]) -> Result<T, ChatError> {
    if plaintext.len() > MAX_SNAPSHOT_BYTES {
        return Err(ChatError::StorageUnavailable);
    }
    let mut remaining = plaintext;
    let state = T::decode_with_depth_limit(
        MAX_DECODE_DEPTH,
        &mut MemTrackingInput::new(&mut remaining, MAX_DECODE_BYTES),
    )
    .map_err(|_| ChatError::StorageUnavailable)?;
    // DecodeAll's exact-consumption rule, with both depth and memory tracking
    // applied to the same decode (DecodeAll itself only accepts a byte slice).
    if !remaining.is_empty() {
        return Err(ChatError::StorageUnavailable);
    }
    Ok(state)
}
