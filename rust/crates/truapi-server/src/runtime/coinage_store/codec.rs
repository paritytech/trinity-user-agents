// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded snapshot format for the Brevity Coinage repository contracts.

use std::collections::BTreeSet;

use parity_scale_codec::{Decode, Encode, Output};
use truapi_coinage::{
    claim_plan::{ClaimPlan, ClaimPlanStatus, CodableClaimPlanEntry},
    model::{Coin, CoinState, Voucher, VoucherLocalState, VoucherPrivacyLevel, VoucherRemoteState},
    wal::{CheckpointBlock, TransferWalEntry, WalCoinRef, WalOperation, WalPayload},
};
use zeroize::Zeroizing;

use super::{
    MAX_ASSETS, MAX_ID_BYTES, MAX_ITEMS, MAX_OPERATION_BYTES, MAX_RECORDS, MAX_SNAPSHOT_BYTES,
    Snapshot, StoreError,
};

fn bound(len: usize, max: usize) -> Result<(), StoreError> {
    if len > max {
        Err(StoreError::Capacity)
    } else {
        Ok(())
    }
}

pub(super) fn validate_id(id: &str) -> Result<(), StoreError> {
    bound(id.len(), MAX_ID_BYTES)?;
    if id.is_empty() {
        Err(StoreError::Corrupt)
    } else {
        Ok(())
    }
}

pub(super) fn advance(counter: &mut Option<u32>, index: u32) {
    *counter = Some(counter.map_or(index, |old| old.max(index)));
}

fn within(counter: Option<u32>, index: u32) -> Result<(), StoreError> {
    if counter.is_some_and(|high| index <= high) {
        Ok(())
    } else {
        Err(StoreError::Corrupt)
    }
}

fn references(refs: &[WalCoinRef], counter: Option<u32>) -> Result<(), StoreError> {
    bound(refs.len(), MAX_ITEMS)?;
    let mut seen = BTreeSet::new();
    for item in refs {
        within(counter, item.derivation_index)?;
        if !seen.insert(item.derivation_index) {
            return Err(StoreError::Corrupt);
        }
    }
    Ok(())
}

pub(super) fn validate(state: &Snapshot) -> Result<(), StoreError> {
    bound(state.coins.len(), MAX_ASSETS)?;
    bound(state.vouchers.len(), MAX_ASSETS)?;
    bound(state.wal.len(), MAX_RECORDS)?;
    bound(state.plans.len(), MAX_RECORDS)?;
    bound(state.operations.len(), MAX_RECORDS)?;
    for (index, coin) in &state.coins {
        within(state.coin_index, *index)?;
        if *index != coin.derivation_index || coin.age.is_some_and(|age| age < 0) {
            return Err(StoreError::Corrupt);
        }
    }
    for (index, voucher) in &state.vouchers {
        within(state.voucher_index, *index)?;
        if *index != voucher.derivation_index || voucher.ready_at_ms < voucher.allocated_at_ms {
            return Err(StoreError::Corrupt);
        }
    }
    for (id, entry) in &state.wal {
        validate_id(id)?;
        if id != &entry.entry_id {
            return Err(StoreError::Corrupt);
        }
        let payload = &entry.payload;
        references(&payload.input_coins, state.coin_index)?;
        references(&payload.input_vouchers, state.voucher_index)?;
        references(&payload.output_coins, state.coin_index)?;
        references(&payload.output_vouchers, state.voucher_index)?;
        references(&payload.destination_coins, state.coin_index)?;
        let outputs: std::collections::BTreeMap<_, _> = payload
            .output_coins
            .iter()
            .map(|coin| (coin.derivation_index, coin.exponent))
            .collect();
        if payload
            .destination_coins
            .iter()
            .any(|coin| outputs.get(&coin.derivation_index) != Some(&coin.exponent))
        {
            return Err(StoreError::Corrupt);
        }
        let inputs: BTreeSet<_> = payload
            .input_coins
            .iter()
            .map(|coin| coin.derivation_index)
            .collect();
        if payload
            .output_coins
            .iter()
            .any(|coin| inputs.contains(&coin.derivation_index))
        {
            return Err(StoreError::Corrupt);
        }
        let inputs: BTreeSet<_> = payload
            .input_vouchers
            .iter()
            .map(|coin| coin.derivation_index)
            .collect();
        if payload
            .output_vouchers
            .iter()
            .any(|coin| inputs.contains(&coin.derivation_index))
        {
            return Err(StoreError::Corrupt);
        }
        for coin in payload.input_coins.iter().chain(&payload.output_coins) {
            if state
                .coins
                .get(&coin.derivation_index)
                .is_some_and(|row| row.exponent != coin.exponent)
            {
                return Err(StoreError::Corrupt);
            }
        }
        for voucher in payload
            .input_vouchers
            .iter()
            .chain(&payload.output_vouchers)
        {
            if state
                .vouchers
                .get(&voucher.derivation_index)
                .is_some_and(|row| row.exponent != voucher.exponent)
            {
                return Err(StoreError::Corrupt);
            }
        }
    }
    for (key, plan) in &state.plans {
        if key != &plan.memo_key {
            return Err(StoreError::Corrupt);
        }
        if let Some(id) = &plan.message_id {
            validate_id(id)?;
        }
        bound(plan.entries.len(), MAX_ITEMS)?;
        bound(plan.outgoing_public_keys.len(), MAX_ITEMS)?;
        let mut entry_ids = BTreeSet::new();
        let mut indices = BTreeSet::new();
        for entry in &plan.entries {
            within(state.coin_index, entry.derivation_index)?;
            if entry.entry_index < 0
                || !entry_ids.insert(entry.entry_index)
                || !indices.insert(entry.derivation_index)
            {
                return Err(StoreError::Corrupt);
            }
            if state
                .coins
                .get(&entry.derivation_index)
                .is_some_and(|row| row.exponent != entry.exponent)
            {
                return Err(StoreError::Corrupt);
            }
        }
        let keys: BTreeSet<_> = plan.outgoing_public_keys.iter().collect();
        if keys.len() != plan.outgoing_public_keys.len() {
            return Err(StoreError::Corrupt);
        }
    }
    for bytes in state.operations.values() {
        bound(bytes.len(), MAX_OPERATION_BYTES)?;
    }
    Ok(())
}

fn count(out: &mut impl Output, len: usize) {
    (len as u32).encode_to(out);
}
fn bytes(out: &mut impl Output, value: &[u8]) {
    count(out, value.len());
    out.write(value);
}
fn refs(out: &mut impl Output, values: &[WalCoinRef]) {
    count(out, values.len());
    for item in values {
        item.derivation_index.encode_to(out);
        item.exponent.encode_to(out);
    }
}

fn encode_to(state: &Snapshot, out: &mut impl Output) {
    state.asset_instance.encode_to(out);
    state.coin_index.encode_to(out);
    state.voucher_index.encode_to(out);
    count(out, state.coins.len());
    for coin in state.coins.values() {
        coin.derivation_index.encode_to(out);
        coin.exponent.encode_to(out);
        coin.age.encode_to(out);
        coin.state.as_raw().encode_to(out);
    }
    count(out, state.vouchers.len());
    for voucher in state.vouchers.values() {
        voucher.derivation_index.encode_to(out);
        voucher.exponent.encode_to(out);
        voucher.allocated_at_ms.encode_to(out);
        voucher.ready_at_ms.encode_to(out);
        let (remote, recycler) = match voucher.remote_state {
            VoucherRemoteState::Unlocated => (0u8, None),
            VoucherRemoteState::Onboarding => (1, None),
            VoucherRemoteState::InRecycler { recycler_index } => (2, Some(recycler_index)),
            VoucherRemoteState::Unloaded => (3, None),
        };
        remote.encode_to(out);
        recycler.encode_to(out);
        voucher.local_state.as_raw().encode_to(out);
        match voucher.privacy {
            VoucherPrivacyLevel::Degraded => 0u8,
            VoucherPrivacyLevel::Full => 1,
        }
        .encode_to(out);
    }
    count(out, state.wal.len());
    for entry in state.wal.values() {
        bytes(out, entry.entry_id.as_bytes());
        entry.operation.as_raw().encode_to(out);
        refs(out, &entry.payload.input_coins);
        refs(out, &entry.payload.input_vouchers);
        refs(out, &entry.payload.output_coins);
        refs(out, &entry.payload.output_vouchers);
        refs(out, &entry.payload.destination_coins);
        let checkpoint = match entry.checkpoint {
            CheckpointBlock::Pending => None,
            CheckpointBlock::Known { number, hash } => Some((number, hash)),
        };
        checkpoint.encode_to(out);
        entry.created_at_ms.encode_to(out);
    }
    count(out, state.plans.len());
    for plan in state.plans.values() {
        plan.memo_key.encode_to(out);
        plan.message_id.is_some().encode_to(out);
        if let Some(id) = &plan.message_id {
            bytes(out, id.as_bytes());
        }
        count(out, plan.entries.len());
        for entry in &plan.entries {
            entry.entry_index.encode_to(out);
            entry.exponent.encode_to(out);
            entry.derivation_index.encode_to(out);
        }
        count(out, plan.outgoing_public_keys.len());
        for key in &plan.outgoing_public_keys {
            key.encode_to(out);
        }
        plan.detection_anchor.encode_to(out);
        plan.status.as_raw().encode_to(out);
        plan.claimed_amount.encode_to(out);
        plan.total_value.encode_to(out);
    }
    count(out, state.operations.len());
    for (id, value) in &state.operations {
        id.encode_to(out);
        bytes(out, value);
    }
}

struct Size(usize);
impl Output for Size {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = self.0.saturating_add(bytes.len());
    }
}

pub(super) fn encode(state: &Snapshot, spare: usize) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    let mut size = Size(0);
    encode_to(state, &mut size);
    bound(size.0, MAX_SNAPSHOT_BYTES)?;
    // Size first: growing a secret-bearing Vec could free uncleared allocations.
    let mut output = Zeroizing::new(Vec::with_capacity(size.0 + spare));
    encode_to(state, &mut *output);
    Ok(output)
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    // Only fixed-size primitives and options/tuples thereof use generic Decode.
    fn value<T: Decode>(&mut self) -> Result<T, StoreError> {
        T::decode(&mut self.0).map_err(|_| StoreError::Corrupt)
    }
    fn count(&mut self, max: usize) -> Result<usize, StoreError> {
        let value = self.value::<u32>()? as usize;
        bound(value, max)?;
        Ok(value)
    }
    fn bytes(&mut self, max: usize) -> Result<&[u8], StoreError> {
        let len = self.count(max)?;
        if len > self.0.len() {
            return Err(StoreError::Corrupt);
        }
        let (value, rest) = self.0.split_at(len);
        self.0 = rest;
        Ok(value)
    }
    fn id(&mut self) -> Result<String, StoreError> {
        let id =
            core::str::from_utf8(self.bytes(MAX_ID_BYTES)?).map_err(|_| StoreError::Corrupt)?;
        validate_id(id)?;
        Ok(id.to_owned())
    }
    fn refs(&mut self) -> Result<Vec<WalCoinRef>, StoreError> {
        let count = self.count(MAX_ITEMS)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(WalCoinRef {
                derivation_index: self.value()?,
                exponent: self.value()?,
            });
        }
        Ok(values)
    }
}

pub(super) fn decode(plaintext: &[u8]) -> Result<Snapshot, StoreError> {
    bound(plaintext.len(), MAX_SNAPSHOT_BYTES)?;
    let mut input = Reader(plaintext);
    let mut state = Snapshot {
        asset_instance: input.value()?,
        coin_index: input.value()?,
        voucher_index: input.value()?,
        ..Snapshot::default()
    };
    for _ in 0..input.count(MAX_ASSETS)? {
        let coin = Coin {
            derivation_index: input.value()?,
            exponent: input.value()?,
            age: input.value()?,
            state: CoinState::from_raw(input.value()?).ok_or(StoreError::Corrupt)?,
        };
        if state.coins.insert(coin.derivation_index, coin).is_some() {
            return Err(StoreError::Corrupt);
        }
    }
    for _ in 0..input.count(MAX_ASSETS)? {
        let derivation_index = input.value()?;
        let exponent = input.value()?;
        let allocated_at_ms = input.value()?;
        let ready_at_ms = input.value()?;
        let remote = input.value::<u8>()?;
        let recycler = input.value::<Option<u32>>()?;
        let remote_state = match (remote, recycler) {
            (0, None) => VoucherRemoteState::Unlocated,
            (1, None) => VoucherRemoteState::Onboarding,
            (2, Some(recycler_index)) => VoucherRemoteState::InRecycler { recycler_index },
            (3, None) => VoucherRemoteState::Unloaded,
            _ => return Err(StoreError::Corrupt),
        };
        let local_state = VoucherLocalState::from_raw(input.value()?).ok_or(StoreError::Corrupt)?;
        let privacy = match input.value::<u8>()? {
            0 => VoucherPrivacyLevel::Degraded,
            1 => VoucherPrivacyLevel::Full,
            _ => return Err(StoreError::Corrupt),
        };
        let voucher = Voucher {
            derivation_index,
            exponent,
            allocated_at_ms,
            ready_at_ms,
            remote_state,
            local_state,
            privacy,
        };
        if state.vouchers.insert(derivation_index, voucher).is_some() {
            return Err(StoreError::Corrupt);
        }
    }
    for _ in 0..input.count(MAX_RECORDS)? {
        let entry_id = input.id()?;
        let operation = WalOperation::from_raw(input.value()?).ok_or(StoreError::Corrupt)?;
        let payload = WalPayload {
            input_coins: input.refs()?,
            input_vouchers: input.refs()?,
            output_coins: input.refs()?,
            output_vouchers: input.refs()?,
            destination_coins: input.refs()?,
        };
        let checkpoint = match input.value::<Option<(u64, [u8; 32])>>()? {
            None => CheckpointBlock::Pending,
            Some((number, hash)) => CheckpointBlock::Known { number, hash },
        };
        let entry = TransferWalEntry {
            entry_id: entry_id.clone(),
            operation,
            payload,
            checkpoint,
            created_at_ms: input.value()?,
        };
        if state.wal.insert(entry_id, entry).is_some() {
            return Err(StoreError::Corrupt);
        }
    }
    for _ in 0..input.count(MAX_RECORDS)? {
        let memo_key = input.value()?;
        let message_id = if input.value::<bool>()? {
            Some(input.id()?)
        } else {
            None
        };
        let count = input.count(MAX_ITEMS)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(CodableClaimPlanEntry {
                entry_index: input.value()?,
                exponent: input.value()?,
                derivation_index: input.value()?,
            });
        }
        let count = input.count(MAX_ITEMS)?;
        let mut outgoing_public_keys = Vec::with_capacity(count);
        for _ in 0..count {
            outgoing_public_keys.push(input.value()?);
        }
        let plan = ClaimPlan {
            memo_key,
            message_id,
            entries,
            outgoing_public_keys,
            detection_anchor: input.value()?,
            status: ClaimPlanStatus::from_raw(input.value()?).ok_or(StoreError::Corrupt)?,
            claimed_amount: input.value()?,
            total_value: input.value()?,
        };
        if state.plans.insert(memo_key, plan).is_some() {
            return Err(StoreError::Corrupt);
        }
    }
    for _ in 0..input.count(MAX_RECORDS)? {
        let id = input.value()?;
        let bytes = Zeroizing::new(input.bytes(MAX_OPERATION_BYTES)?.to_vec());
        if state.operations.insert(id, bytes).is_some() {
            return Err(StoreError::Corrupt);
        }
    }
    if !input.0.is_empty() {
        return Err(StoreError::Corrupt);
    }
    validate(&state)?;
    Ok(state)
}
