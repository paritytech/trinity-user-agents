// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! Chain-backed claiming of externally supplied Coinage secrets.
//! [`ExternalMemoClaiming`] is the secret-preserving claim effect. This module implements that effect without ever projecting a
//! secret through FFI:
//! 1. validate every expanded sr25519 secret and reject duplicate sources;
//! 2. read the live Coinage denomination context and source/destination
//!    state in ordered batches;
//! 3. allocate one session-derived destination with the same denomination;
//! 4. treat a destination present on-chain while its source is absent as
//!    already complete; the inverse is safe to retry. Both present is a
//!    recipient collision and both absent is ambiguous, so both cases fail
//!    closed;
//! 5. submit one `Coinage.transfer` per source under its own `AsCoin`
//!    origin; and
//! 6. save only destinations whose finalized on-chain value was verified.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::COIN_MAX_AGE;
use crate::allocator::CoinAllocator;
use crate::claim_plan::{ClaimPlan, ClaimPlanStatus, ClaimPlanStore, CodableClaimPlanEntry};
use crate::constants::SEND_VERIFY_BLOCK_TIMEOUT;
use crate::denomination::DenominationBreakdownContext;
use crate::keys::CoinKeypairFactory;
use crate::memo::{MemoEntry, TransferMemo};
use crate::model::{Coin, CoinState};
use crate::repo::CoinRepository;
use crate::sync::OnChainCoin;

type SecretSource = Option<(MemoEntry, [u8; 32])>;

/// One source-coin transfer. This type intentionally has no `Debug`
/// implementation: `source_secret` is seed-phrase-tier material.
pub struct ExternalCoinTransferRequest {
    pub source_secret: MemoEntry,
    pub source_public: [u8; 32],
    pub recipient: [u8; 32],
    pub exponent: i16,
    pub asset_unit: u128,
    pub amount_planks: u128,
}

/// The chain edge used by [`ExternalSecretClaimService`].
/// A production implementation must revalidate the request against live
/// metadata/state immediately before signing and return only after the
/// submitted extrinsic finalized successfully and the destination coin was
/// observed with the requested exponent.
#[async_trait]
pub trait ExternalCoinTransferBackend: Send + Sync {
    async fn denomination_context(&self) -> Result<DenominationBreakdownContext, String>;

    /// Ordered `Coinage.CoinsByOwner` batch.
    async fn fetch_coins(
        &self,
        public_keys: &[[u8; 32]],
    ) -> Result<Vec<Option<OnChainCoin>>, String>;

    /// Consumes the raw secret and returns the finalized destination row.
    async fn submit_transfer(
        &self,
        request: ExternalCoinTransferRequest,
    ) -> Result<OnChainCoin, String>;
}

/// Session-scoped recipient for W3S/external Coinage secrets.
/// Construct a fresh instance from the active wallet entropy. The
/// destination key factory owns no FFI-visible surface and zeroizes its root
/// material on drop.
pub struct ExternalSecretClaimService {
    key_factory: Arc<CoinKeypairFactory>,
    allocator: Arc<CoinAllocator>,
    coins: Arc<dyn CoinRepository>,
    plans: Arc<dyn ClaimPlanStore>,
    backend: Arc<dyn ExternalCoinTransferBackend>,
    operation: Mutex<()>,
}

impl ExternalSecretClaimService {
    pub fn new(
        root_entropy: &[u8],
        allocator: Arc<CoinAllocator>,
        coins: Arc<dyn CoinRepository>,
        plans: Arc<dyn ClaimPlanStore>,
        backend: Arc<dyn ExternalCoinTransferBackend>,
    ) -> Self {
        Self {
            key_factory: Arc::new(CoinKeypairFactory::new(root_entropy)),
            allocator,
            coins,
            plans,
            backend,
            operation: Mutex::new(()),
        }
    }

    pub async fn await_memo_sources_on_chain(&self, memo: &TransferMemo) -> Result<(), String> {
        self.await_memo_sources_on_chain_with(
            memo,
            SEND_VERIFY_BLOCK_TIMEOUT,
            std::time::Duration::from_secs(6),
        )
        .await
    }

    /// Timeout-parameterized body of [`Self::await_memo_sources_on_chain`]
    /// (attempt count ≙ finalized blocks, interval ≙ block time).
    pub async fn await_memo_sources_on_chain_with(
        &self,
        memo: &TransferMemo,
        attempts: u32,
        interval: std::time::Duration,
    ) -> Result<(), String> {
        if memo.entries.is_empty() {
            return Err("external coin claim requires at least one secret".into());
        }
        let mut publics = Vec::with_capacity(memo.entries.len());
        for (index, entry) in memo.entries.iter().enumerate() {
            let public = schnorrkel::SecretKey::from_bytes(&entry.0)
                .map_err(|error| format!("external coin secret {index} is invalid: {error}"))?
                .to_public()
                .to_bytes();
            publics.push(public);
        }
        for attempt in 0..attempts.max(1) {
            if attempt > 0 {
                crate::timer::sleep(interval).await;
            }
            let rows =
                fetch_exact(self.backend.as_ref(), &publics, "external send detection").await?;
            if rows.iter().all(Option::is_some) {
                return Ok(());
            }
        }
        Err(format!(
            "external send not detected within {} blocks",
            attempts.max(1)
        ))
    }

    /// Validate and persist the complete destination plan without submitting
    /// any transaction. Hosts call this before acknowledging custody of a memo.
    pub async fn prepare_memo(
        &self,
        memo: &TransferMemo,
        message_id: String,
    ) -> Result<ClaimPlan, String> {
        let _operation = self.operation.lock().await;
        self.prepare_locked(memo, message_id).await
    }

    async fn prepare_locked(
        &self,
        memo: &TransferMemo,
        message_id: String,
    ) -> Result<ClaimPlan, String> {
        if memo.entries.is_empty() {
            return Err("external coin claim requires at least one secret".into());
        }
        let memo_key = memo.identifier();

        // A Finished plan is authoritative: this memo was claimed on a
        // prior run. Report the stored amount instead of re-executing —
        // the claimed coins may since have been spent or recycled, so
        // re-verification against live chain state can no longer prove
        // anything and must not overwrite the terminal outcome.
        let existing = self.plans.plan(&memo_key).await?;
        if let Some(plan) = &existing
            && plan.status == ClaimPlanStatus::Finished
        {
            if plan.memo_key != memo_key
                || plan.total_value != memo.total_value
                || plan.claimed_amount != Some(memo.total_value)
                || plan.entries.len() != memo.entries.len()
            {
                return Err("finished external coin claim does not match its memo".into());
            }
            return Ok(plan.clone());
        }

        let context = self.backend.denomination_context().await?;
        validate_context(&context)?;

        let total_value = memo.total_value;
        let sources = source_entries(memo.entries.clone())?;
        let plan = match existing {
            Some(plan) => {
                validate_existing_plan(
                    &plan,
                    &memo_key,
                    &message_id,
                    total_value,
                    sources.len(),
                    &context,
                )?;
                plan
            }
            None => {
                let source_public = sources
                    .iter()
                    .map(|source| source.as_ref().expect("source installed").1)
                    .collect::<Vec<_>>();
                let source_rows = fetch_exact(
                    self.backend.as_ref(),
                    &source_public,
                    "external source coin query",
                )
                .await?;
                let mut exponents = Vec::with_capacity(source_rows.len());
                let mut computed_total = 0u128;
                for (index, row) in source_rows.into_iter().enumerate() {
                    let row = row.ok_or_else(|| {
                        format!("external source coin {index} is no longer on-chain")
                    })?;
                    validate_exponent(row.exponent, &context)?;
                    computed_total = computed_total
                        .checked_add(context.value_in_planks(row.exponent))
                        .ok_or_else(|| "external coin claim total exceeds u128".to_string())?;
                    exponents.push(row.exponent);
                }
                if computed_total != total_value {
                    return Err(format!(
                        "external coin claim amount mismatch: memo {total_value}, chain {computed_total}"
                    ));
                }

                let plan = self
                    .allocate_plan(
                        memo_key,
                        message_id.clone(),
                        total_value,
                        &source_public,
                        &exponents,
                    )
                    .await?;
                self.plans.save(&plan).await?;
                plan
            }
        };

        Ok(plan)
    }

    async fn claim(&self, mut memo: TransferMemo, message_id: String) -> Result<u128, String> {
        let _operation = self.operation.lock().await;
        let plan = self.prepare_locked(&memo, message_id).await?;
        if plan.status == ClaimPlanStatus::Finished {
            return Ok(plan.claimed_amount.unwrap_or(plan.total_value));
        }
        let memo_key = plan.memo_key;
        let context = self.backend.denomination_context().await?;
        validate_context(&context)?;
        let mut sources = source_entries(std::mem::take(&mut memo.entries))?;
        let outcome = self.execute_plan(&plan, &context, &mut sources).await;
        match outcome {
            Ok(claimed) => {
                self.plans
                    .update_status(&memo_key, ClaimPlanStatus::Finished, Some(claimed))
                    .await?;
                Ok(claimed)
            }
            Err(error) => {
                let confirmed = self
                    .plans
                    .plan(&memo_key)
                    .await?
                    .and_then(|plan| plan.claimed_amount);
                if let Err(status_error) = self
                    .plans
                    .update_status(&memo_key, ClaimPlanStatus::Error, confirmed)
                    .await
                {
                    tracing::warn!(
                        %status_error,
                        "external coin claim error status could not be persisted"
                    );
                }
                Err(error)
            }
        }
    }

    async fn allocate_plan(
        &self,
        memo_key: [u8; 32],
        message_id: String,
        total_value: u128,
        source_public: &[[u8; 32]],
        exponents: &[i16],
    ) -> Result<ClaimPlan, String> {
        let mut entries = Vec::with_capacity(exponents.len());
        let mut recipients = Vec::with_capacity(exponents.len());
        let source_set = source_public.iter().copied().collect::<HashSet<_>>();
        let mut recipient_set = HashSet::with_capacity(exponents.len());

        for (entry_index, exponent) in exponents.iter().copied().enumerate() {
            let entry_index = i16::try_from(entry_index)
                .map_err(|_| "external coin claim has too many entries".to_string())?;
            let destination = self.allocator.allocate(exponent).await?;
            let recipient = self.key_factory.public_key(destination.derivation_index)?;
            if recipient == [0; 32]
                || source_set.contains(&recipient)
                || !recipient_set.insert(recipient)
            {
                return Err("external coin claim destination key collision".into());
            }
            recipients.push(recipient);
            entries.push(CodableClaimPlanEntry {
                entry_index,
                exponent,
                derivation_index: destination.derivation_index,
            });
        }

        let destination_rows = fetch_exact(
            self.backend.as_ref(),
            &recipients,
            "external destination collision query",
        )
        .await?;
        if destination_rows.iter().any(Option::is_some) {
            return Err("external coin claim destination already exists on-chain".into());
        }

        Ok(ClaimPlan {
            memo_key,
            message_id: Some(message_id),
            entries,
            outgoing_public_keys: Vec::new(),
            detection_anchor: None,
            status: ClaimPlanStatus::Processing,
            claimed_amount: None,
            total_value,
        })
    }

    async fn execute_plan(
        &self,
        plan: &ClaimPlan,
        context: &DenominationBreakdownContext,
        sources: &mut [SecretSource],
    ) -> Result<u128, String> {
        let entries = ordered_plan_entries(plan, sources.len())?;
        let source_public = sources
            .iter()
            .map(|source| source.as_ref().expect("validated source").1)
            .collect::<Vec<_>>();
        let recipients = entries
            .iter()
            .map(|entry| self.key_factory.public_key(entry.derivation_index))
            .collect::<Result<Vec<_>, _>>()?;
        validate_recipients(&source_public, &recipients)?;

        let mut all_keys = source_public.clone();
        all_keys.extend_from_slice(&recipients);
        let mut rows = fetch_exact(
            self.backend.as_ref(),
            &all_keys,
            "external claim recovery query",
        )
        .await?;
        let destination_rows = rows.split_off(source_public.len());
        let source_rows = rows;
        let local_states = self
            .coins
            .list()
            .await?
            .into_iter()
            .map(|coin| (coin.derivation_index, coin.state))
            .collect::<HashMap<_, _>>();

        let mut claimed = 0u128;
        for (position, entry) in entries.into_iter().enumerate() {
            validate_exponent(entry.exponent, context)?;
            let expected_amount = context.value_in_planks(entry.exponent);
            claimed = claimed
                .checked_add(expected_amount)
                .ok_or_else(|| "external coin claim total exceeds u128".to_string())?;
            // Each sequential prefix was durably recorded only after exact
            // finalized destination verification. Missing both keys without
            // that evidence is ambiguous, never proof of a successful claim.
            if claimed <= plan.claimed_amount.unwrap_or(0) {
                continue;
            }
            let source = source_rows[position];
            let destination = destination_rows[position];
            let landed = match (source, destination) {
                (None, Some(destination)) => {
                    if destination.exponent != entry.exponent {
                        return Err(format!(
                            "external claim destination {position} denomination mismatch: expected {}, found {}",
                            entry.exponent, destination.exponent
                        ));
                    }
                    destination
                }
                (Some(source), None) => {
                    if source.exponent != entry.exponent {
                        return Err(format!(
                            "external claim source {position} denomination changed: expected {}, found {}",
                            entry.exponent, source.exponent
                        ));
                    }
                    let (source_secret, source_public) = sources[position]
                        .take()
                        .expect("validated source entry consumed once");
                    let recipient = recipients[position];
                    self.backend
                        .submit_transfer(ExternalCoinTransferRequest {
                            source_secret,
                            source_public,
                            recipient,
                            exponent: entry.exponent,
                            asset_unit: context.asset_unit,
                            amount_planks: expected_amount,
                        })
                        .await?
                }
                (Some(_), Some(_)) => {
                    return Err(
                        "external claim destination collision: source remains unconsumed".into(),
                    );
                }
                (None, None) => {
                    return Err("external claim source and destination are both absent without finalized evidence".into());
                }
            };
            if landed.exponent != entry.exponent {
                return Err(format!(
                    "external claim finalized destination {position} denomination mismatch: expected {}, found {}",
                    entry.exponent, landed.exponent
                ));
            }
            let state = local_states
                .get(&entry.derivation_index)
                .copied()
                .unwrap_or(CoinState::Available);
            self.coins
                .upsert(&Coin {
                    exponent: entry.exponent,
                    derivation_index: entry.derivation_index,
                    age: Some(landed.age),
                    state,
                })
                .await?;
            self.plans
                .update_status(&plan.memo_key, ClaimPlanStatus::Processing, Some(claimed))
                .await?;
        }
        if claimed != plan.total_value {
            return Err(format!(
                "external coin claim plan amount mismatch: plan {}, entries {claimed}",
                plan.total_value
            ));
        }
        Ok(claimed)
    }
}

#[async_trait]
impl ExternalMemoClaiming for ExternalSecretClaimService {
    async fn claim_external_memo(
        &self,
        memo: TransferMemo,
        message_id: String,
    ) -> Result<u128, String> {
        self.claim(memo, message_id).await
    }
}

/// Exact result of one local spent-coin recovery pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpentCoinTransferRecoveryReport {
    /// Present max-age coins restored locally plus younger coins moved to
    /// fresh destinations.
    pub recovered_count: u32,
    pub recovered_planks: u128,
    /// Subset restored locally because `Coinage.transfer` would reject a
    /// coin at or beyond `COIN_MAX_AGE`.
    pub restored_max_age_count: u32,
    /// Subset claimed through finalized `Coinage.transfer` extrinsics.
    pub transferred_count: u32,
}

pub struct SpentCoinTransferRecoveryService {
    key_factory: Arc<CoinKeypairFactory>,
    coins: Arc<dyn CoinRepository>,
    backend: Arc<dyn ExternalCoinTransferBackend>,
    claimer: Arc<ExternalSecretClaimService>,
    operation: Mutex<()>,
}

impl SpentCoinTransferRecoveryService {
    pub fn new(
        root_entropy: &[u8],
        coins: Arc<dyn CoinRepository>,
        backend: Arc<dyn ExternalCoinTransferBackend>,
        claimer: Arc<ExternalSecretClaimService>,
    ) -> Self {
        Self {
            key_factory: Arc::new(CoinKeypairFactory::new(root_entropy)),
            coins,
            backend,
            claimer,
            operation: Mutex::new(()),
        }
    }

    pub async fn recover(
        &self,
        spent: Vec<Coin>,
    ) -> Result<SpentCoinTransferRecoveryReport, String> {
        let _operation = self.operation.lock().await;
        if spent.is_empty() {
            return Ok(SpentCoinTransferRecoveryReport::default());
        }
        let mut indices = HashSet::with_capacity(spent.len());
        for coin in &spent {
            if coin.state != CoinState::Spent {
                return Err(format!(
                    "spent recovery received non-spent coin {}",
                    coin.derivation_index
                ));
            }
            if !indices.insert(coin.derivation_index) {
                return Err(format!(
                    "spent recovery coin {} is duplicated",
                    coin.derivation_index
                ));
            }
        }

        let context = self.backend.denomination_context().await?;
        validate_context(&context)?;
        let public_keys = spent
            .iter()
            .map(|coin| self.key_factory.public_key(coin.derivation_index))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = fetch_exact(
            self.backend.as_ref(),
            &public_keys,
            "spent Coinage recovery query",
        )
        .await?;

        let mut report = SpentCoinTransferRecoveryReport::default();
        let mut transfer_entries = Vec::new();
        let mut transfer_total = 0u128;
        for (coin, row) in spent.into_iter().zip(rows) {
            let Some(row) = row else {
                continue;
            };
            validate_exponent(row.exponent, &context)?;
            if row.exponent != coin.exponent {
                return Err(format!(
                    "spent coin {} denomination mismatch: local {}, chain {}",
                    coin.derivation_index, coin.exponent, row.exponent
                ));
            }
            let value = context.value_in_planks(row.exponent);
            if row.age >= COIN_MAX_AGE {
                self.coins
                    .upsert(&Coin {
                        exponent: row.exponent,
                        derivation_index: coin.derivation_index,
                        age: Some(row.age),
                        state: CoinState::Available,
                    })
                    .await?;
                report.recovered_count = report.recovered_count.saturating_add(1);
                report.restored_max_age_count = report.restored_max_age_count.saturating_add(1);
                report.recovered_planks = report
                    .recovered_planks
                    .checked_add(value)
                    .ok_or_else(|| "spent Coinage recovery total exceeds u128".to_string())?;
            } else {
                transfer_entries.push(MemoEntry(
                    self.key_factory.secret_bytes(coin.derivation_index)?,
                ));
                transfer_total = transfer_total
                    .checked_add(value)
                    .ok_or_else(|| "spent Coinage transfer total exceeds u128".to_string())?;
                report.transferred_count = report.transferred_count.saturating_add(1);
            }
        }

        if !transfer_entries.is_empty() {
            let memo = TransferMemo {
                entries: transfer_entries,
                total_value: transfer_total,
            };
            let memo_key = memo.identifier();
            self.claimer
                .claim_external_memo(memo, external_claim_message_id(&memo_key))
                .await?;
            report.recovered_count = report
                .recovered_count
                .checked_add(report.transferred_count)
                .ok_or_else(|| "spent Coinage recovered count exceeds u32".to_string())?;
            report.recovered_planks = report
                .recovered_planks
                .checked_add(transfer_total)
                .ok_or_else(|| "spent Coinage recovery total exceeds u128".to_string())?;
        }
        Ok(report)
    }
}

#[async_trait]
impl SpentCoinsRecovering for SpentCoinTransferRecoveryService {
    async fn recover_spent_coins(&self, spent: Vec<Coin>) -> Result<u128, String> {
        self.recover(spent)
            .await
            .map(|report| report.recovered_planks)
    }
}

/// The only accepted external-claim id. It is public for host composition
/// and tests, but contains no secret material.
pub fn external_claim_message_id(memo_key: &[u8; 32]) -> String {
    format!("w3s-coins-{}", hex::encode(memo_key))
}

fn source_entries(entries: Vec<MemoEntry>) -> Result<Vec<SecretSource>, String> {
    let mut seen = HashSet::with_capacity(entries.len());
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let secret = schnorrkel::SecretKey::from_bytes(&entry.0)
                .map_err(|error| format!("external coin secret {index} is invalid: {error}"))?;
            let public = secret.to_public().to_bytes();
            if !seen.insert(public) {
                return Err(format!("external coin source {index} is duplicated"));
            }
            Ok(Some((entry, public)))
        })
        .collect()
}

fn validate_context(context: &DenominationBreakdownContext) -> Result<(), String> {
    if context.asset_unit == 0 {
        return Err("Coinage asset unit must be non-zero".into());
    }
    if context.max_exponent < context.min_exponent {
        return Err("Coinage denomination range is inverted".into());
    }
    Ok(())
}

fn validate_exponent(exponent: i16, context: &DenominationBreakdownContext) -> Result<(), String> {
    if exponent < context.min_exponent || exponent > context.max_exponent {
        return Err(format!(
            "Coinage exponent {exponent} is outside {}..={}",
            context.min_exponent, context.max_exponent
        ));
    }
    // The live pallet's transfer preserves its signed i8 `value`.
    i8::try_from(exponent)
        .map(|_| ())
        .map_err(|_| format!("Coinage exponent {exponent} does not fit the runtime i8"))
}

fn validate_existing_plan(
    plan: &ClaimPlan,
    memo_key: &[u8; 32],
    message_id: &str,
    total_value: u128,
    entry_count: usize,
    context: &DenominationBreakdownContext,
) -> Result<(), String> {
    // Plans written before chat claims switched to chat message ids carry
    // the memo-derived external id; accept either spelling.
    let message_id_matches = plan.message_id.as_deref() == Some(message_id)
        || plan.message_id.as_deref() == Some(external_claim_message_id(memo_key).as_str());
    if plan.memo_key != *memo_key || !message_id_matches || plan.total_value != total_value {
        return Err("persisted external coin claim plan does not match its memo".into());
    }
    if plan.entries.len() != entry_count {
        return Err(format!(
            "persisted external coin claim plan has {} entries; expected {entry_count}",
            plan.entries.len()
        ));
    }
    let entries = ordered_plan_entries(plan, entry_count)?;
    let mut total = 0u128;
    let confirmed = plan.claimed_amount.unwrap_or(0);
    let mut confirmed_boundary = confirmed == 0;
    for entry in entries {
        validate_exponent(entry.exponent, context)?;
        total = total
            .checked_add(context.value_in_planks(entry.exponent))
            .ok_or_else(|| "external coin claim plan total exceeds u128".to_string())?;
        confirmed_boundary |= confirmed == total;
    }
    if total != total_value {
        return Err(format!(
            "persisted external coin claim amount mismatch: memo {total_value}, plan {total}"
        ));
    }
    if !confirmed_boundary {
        return Err("persisted external claim progress is not a finalized entry boundary".into());
    }
    Ok(())
}

fn ordered_plan_entries(
    plan: &ClaimPlan,
    entry_count: usize,
) -> Result<Vec<CodableClaimPlanEntry>, String> {
    let mut ordered = vec![None; entry_count];
    for entry in &plan.entries {
        let index = usize::try_from(entry.entry_index)
            .map_err(|_| "external coin claim plan has a negative entry index".to_string())?;
        let slot = ordered.get_mut(index).ok_or_else(|| {
            format!("external coin claim plan entry index {index} is out of bounds")
        })?;
        if slot.replace(*entry).is_some() {
            return Err(format!(
                "external coin claim plan entry index {index} is duplicated"
            ));
        }
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.ok_or_else(|| format!("external coin claim plan entry index {index} is missing"))
        })
        .collect()
}

fn validate_recipients(sources: &[[u8; 32]], recipients: &[[u8; 32]]) -> Result<(), String> {
    if sources.len() != recipients.len() {
        return Err("external coin source/recipient count mismatch".into());
    }
    let source_set = sources.iter().copied().collect::<HashSet<_>>();
    let mut recipient_set = HashSet::with_capacity(recipients.len());
    for recipient in recipients {
        if *recipient == [0; 32]
            || source_set.contains(recipient)
            || !recipient_set.insert(*recipient)
        {
            return Err("external coin claim recipient key collision".into());
        }
    }
    Ok(())
}

async fn fetch_exact(
    backend: &dyn ExternalCoinTransferBackend,
    keys: &[[u8; 32]],
    label: &str,
) -> Result<Vec<Option<OnChainCoin>>, String> {
    let rows = backend.fetch_coins(keys).await?;
    if rows.len() != keys.len() {
        return Err(format!(
            "{label} returned {} values for {} keys",
            rows.len(),
            keys.len()
        ));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex as StdMutex;

    use super::*;
    use crate::index_store::InMemoryCoinageIndexStore;
    use crate::repo::InMemoryCoinRepository;

    fn context() -> DenominationBreakdownContext {
        DenominationBreakdownContext {
            asset_unit: 10,
            max_exponent: 4,
            min_exponent: 0,
            precision: 2,
        }
    }

    fn expanded_secret(seed: u8) -> MemoEntry {
        MemoEntry(
            schnorrkel::MiniSecretKey::from_bytes(&[seed; 32])
                .unwrap()
                .expand(schnorrkel::ExpansionMode::Ed25519)
                .to_bytes(),
        )
    }

    fn public(entry: &MemoEntry) -> [u8; 32] {
        schnorrkel::SecretKey::from_bytes(&entry.0)
            .unwrap()
            .to_public()
            .to_bytes()
    }

    #[derive(Default)]
    struct MemoryPlans(StdMutex<HashMap<[u8; 32], ClaimPlan>>);

    #[async_trait]
    impl ClaimPlanStore for MemoryPlans {
        async fn save(&self, plan: &ClaimPlan) -> Result<(), String> {
            self.0.lock().insert(plan.memo_key, plan.clone());
            Ok(())
        }

        async fn plan(&self, memo_key: &[u8; 32]) -> Result<Option<ClaimPlan>, String> {
            Ok(self.0.lock().get(memo_key).cloned())
        }

        async fn load_all(&self) -> Result<Vec<ClaimPlan>, String> {
            Ok(self.0.lock().values().cloned().collect())
        }

        async fn update_status(
            &self,
            memo_key: &[u8; 32],
            status: ClaimPlanStatus,
            claimed_amount: Option<u128>,
        ) -> Result<(), String> {
            let mut plans = self.0.lock();
            let plan = plans
                .get_mut(memo_key)
                .ok_or_else(|| "claim plan is missing".to_string())?;
            plan.status = status;
            plan.claimed_amount = claimed_amount;
            Ok(())
        }

        async fn remove(&self, memo_key: &[u8; 32]) -> Result<(), String> {
            self.0.lock().remove(memo_key);
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedTransfer {
        source: [u8; 32],
        recipient: [u8; 32],
        exponent: i16,
        asset_unit: u128,
        amount_planks: u128,
    }

    struct MockBackend {
        context: DenominationBreakdownContext,
        state: StdMutex<HashMap<[u8; 32], OnChainCoin>>,
        transfers: StdMutex<Vec<RecordedTransfer>>,
        plans: Arc<MemoryPlans>,
        expected_plan: StdMutex<Option<[u8; 32]>>,
    }

    #[async_trait]
    impl ExternalCoinTransferBackend for MockBackend {
        async fn denomination_context(&self) -> Result<DenominationBreakdownContext, String> {
            Ok(self.context.clone())
        }

        async fn fetch_coins(
            &self,
            public_keys: &[[u8; 32]],
        ) -> Result<Vec<Option<OnChainCoin>>, String> {
            let state = self.state.lock();
            Ok(public_keys
                .iter()
                .map(|public| state.get(public).copied())
                .collect())
        }

        async fn submit_transfer(
            &self,
            request: ExternalCoinTransferRequest,
        ) -> Result<OnChainCoin, String> {
            if let Some(expected_plan) = *self.expected_plan.lock() {
                assert!(
                    self.plans.0.lock().contains_key(&expected_plan),
                    "claim plan must be durable before the first transfer"
                );
            }
            let derived = public(&request.source_secret);
            assert_eq!(derived, request.source_public);
            let mut state = self.state.lock();
            let source = state
                .remove(&request.source_public)
                .ok_or_else(|| "source disappeared".to_string())?;
            assert_eq!(source.exponent, request.exponent);
            assert!(!state.contains_key(&request.recipient));
            let landed = OnChainCoin {
                exponent: request.exponent,
                age: 0,
            };
            state.insert(request.recipient, landed);
            self.transfers.lock().push(RecordedTransfer {
                source: request.source_public,
                recipient: request.recipient,
                exponent: request.exponent,
                asset_unit: request.asset_unit,
                amount_planks: request.amount_planks,
            });
            Ok(landed)
        }
    }

    struct Rig {
        service: Arc<ExternalSecretClaimService>,
        plans: Arc<MemoryPlans>,
        coins: Arc<InMemoryCoinRepository>,
        backend: Arc<MockBackend>,
    }

    fn rig(source_rows: impl IntoIterator<Item = ([u8; 32], OnChainCoin)>) -> Rig {
        let plans = Arc::new(MemoryPlans::default());
        let coins = Arc::new(InMemoryCoinRepository::default());
        let backend = Arc::new(MockBackend {
            context: context(),
            state: StdMutex::new(source_rows.into_iter().collect()),
            transfers: StdMutex::new(Vec::new()),
            plans: Arc::clone(&plans),
            expected_plan: StdMutex::new(None),
        });
        let service = Arc::new(ExternalSecretClaimService::new(
            &[0x44; 16],
            Arc::new(CoinAllocator::new(Arc::new(
                InMemoryCoinageIndexStore::default(),
            ))),
            Arc::clone(&coins) as Arc<_>,
            Arc::clone(&plans) as Arc<_>,
            Arc::clone(&backend) as Arc<_>,
        ));
        Rig {
            service,
            plans,
            coins,
            backend,
        }
    }

    #[tokio::test]
    async fn await_memo_sources_waits_until_all_sources_land() {
        let entry_a = expanded_secret(1);
        let entry_b = expanded_secret(2);
        let coin = OnChainCoin {
            exponent: 1,
            age: 0,
        };
        // Only A is on-chain at start; B lands while the wait is polling.
        let rig = rig([(public(&entry_a), coin)]);
        let waiter = {
            let service = Arc::clone(&rig.service);
            let memo = TransferMemo {
                entries: vec![entry_a.clone(), entry_b.clone()],
                total_value: 20,
            };
            tokio::spawn(async move {
                service
                    .await_memo_sources_on_chain_with(
                        &memo,
                        200,
                        std::time::Duration::from_millis(2),
                    )
                    .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        rig.backend.state.lock().insert(public(&entry_b), coin);
        waiter
            .await
            .unwrap()
            .expect("resolves once every source landed");

        // A memo whose sources never land times out into an error.
        let missing = TransferMemo {
            entries: vec![expanded_secret(3)],
            total_value: 10,
        };
        let outcome = rig
            .service
            .await_memo_sources_on_chain_with(&missing, 3, std::time::Duration::from_millis(1))
            .await;
        assert!(outcome.unwrap_err().contains("not detected"));
    }

    #[tokio::test]
    async fn claim_persists_plan_before_exact_denominated_transfers() {
        let first = expanded_secret(1);
        let second = expanded_secret(2);
        let memo = TransferMemo {
            entries: vec![first.clone(), second.clone()],
            total_value: 60,
        };
        let memo_key = memo.identifier();
        let rig = rig([
            (
                public(&first),
                OnChainCoin {
                    exponent: 1,
                    age: 7,
                },
            ),
            (
                public(&second),
                OnChainCoin {
                    exponent: 2,
                    age: 9,
                },
            ),
        ]);
        *rig.backend.expected_plan.lock() = Some(memo_key);

        rig.service
            .claim_external_memo(memo, external_claim_message_id(&memo_key))
            .await
            .unwrap();

        let transfers = rig.backend.transfers.lock().clone();
        assert_eq!(transfers.len(), 2);
        assert_eq!(
            transfers
                .iter()
                .map(|transfer| (
                    transfer.exponent,
                    transfer.asset_unit,
                    transfer.amount_planks,
                ))
                .collect::<Vec<_>>(),
            [(1, 10, 20), (2, 10, 40)]
        );
        let plan = rig.plans.0.lock().get(&memo_key).unwrap().clone();
        assert_eq!(plan.status, ClaimPlanStatus::Finished);
        assert_eq!(plan.claimed_amount, Some(60));
        assert_eq!(plan.entries.len(), 2);
        let coins = rig.coins.list().await.unwrap();
        assert_eq!(coins.len(), 2);
        assert!(coins.iter().all(|coin| coin.state == CoinState::Available));
    }

    #[tokio::test]
    async fn amount_mismatch_never_allocates_or_submits() {
        let source = expanded_secret(3);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 21,
        };
        let memo_key = memo.identifier();
        let rig = rig([(
            public(&source),
            OnChainCoin {
                exponent: 1,
                age: 0,
            },
        )]);

        let error = rig
            .service
            .claim_external_memo(memo, external_claim_message_id(&memo_key))
            .await
            .unwrap_err();
        assert!(error.contains("amount mismatch"));
        assert!(rig.plans.0.lock().is_empty());
        assert!(rig.backend.transfers.lock().is_empty());
    }

    #[tokio::test]
    async fn duplicate_source_fails_before_chain_mutation() {
        let source = expanded_secret(4);
        let duplicate = TransferMemo {
            entries: vec![source.clone(), source.clone()],
            total_value: 40,
        };
        let duplicate_key = duplicate.identifier();
        let rig = rig([(
            public(&source),
            OnChainCoin {
                exponent: 1,
                age: 0,
            },
        )]);
        assert!(
            rig.service
                .claim_external_memo(duplicate, external_claim_message_id(&duplicate_key))
                .await
                .unwrap_err()
                .contains("duplicated")
        );
        assert!(rig.backend.transfers.lock().is_empty());
    }

    /// Chat claims pass the chat row's message id; it must be accepted and
    /// recorded on the plan so startup hydration restores the status under
    /// the id the chat UI actually reads.
    #[tokio::test]
    async fn chat_message_id_is_accepted_and_persisted_on_the_plan() {
        let source = expanded_secret(5);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let memo_key = memo.identifier();
        let rig = rig([(
            public(&source),
            OnChainCoin {
                exponent: 1,
                age: 0,
            },
        )]);

        let claimed = rig
            .service
            .claim_external_memo(memo, "chat-message-1".into())
            .await
            .unwrap();
        assert_eq!(claimed, 20);
        let plan = rig.plans.0.lock().get(&memo_key).unwrap().clone();
        assert_eq!(plan.message_id.as_deref(), Some("chat-message-1"));
        assert_eq!(plan.status, ClaimPlanStatus::Finished);
    }

    #[tokio::test]
    async fn existing_landed_destination_recovers_without_resubmission() {
        let source = expanded_secret(6);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let memo_key = memo.identifier();
        let rig = rig([]);
        let recipient = CoinKeypairFactory::new(&[0x44; 16]).public_key(0).unwrap();
        rig.backend.state.lock().insert(
            recipient,
            OnChainCoin {
                exponent: 1,
                age: 3,
            },
        );
        rig.plans.0.lock().insert(
            memo_key,
            ClaimPlan {
                memo_key,
                message_id: Some(external_claim_message_id(&memo_key)),
                entries: vec![CodableClaimPlanEntry {
                    entry_index: 0,
                    exponent: 1,
                    derivation_index: 0,
                }],
                outgoing_public_keys: Vec::new(),
                detection_anchor: None,
                status: ClaimPlanStatus::Error,
                claimed_amount: None,
                total_value: 20,
            },
        );

        rig.service
            .claim_external_memo(memo, external_claim_message_id(&memo_key))
            .await
            .unwrap();
        assert!(rig.backend.transfers.lock().is_empty());
        let coins = rig.coins.list().await.unwrap();
        assert_eq!(coins.len(), 1);
        assert_eq!(coins[0].age, Some(3));
        assert_eq!(
            rig.plans.0.lock().get(&memo_key).unwrap().status,
            ClaimPlanStatus::Finished
        );
    }

    #[tokio::test]
    async fn prepared_claim_is_durable_without_spending_and_survives_restart() {
        let source = expanded_secret(8);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let key = memo.identifier();
        let rig = rig([(
            public(&source),
            OnChainCoin {
                exponent: 1,
                age: 2,
            },
        )]);
        let plan = rig
            .service
            .prepare_memo(&memo, external_claim_message_id(&key))
            .await
            .unwrap();
        assert!(rig.backend.transfers.lock().is_empty());
        assert_eq!(rig.plans.plan(&key).await.unwrap(), Some(plan.clone()));
        let restarted = ExternalSecretClaimService::new(
            &[0x44; 16],
            Arc::new(CoinAllocator::new(Arc::new(
                InMemoryCoinageIndexStore::default(),
            ))),
            rig.coins.clone(),
            rig.plans.clone(),
            rig.backend.clone(),
        );
        assert_eq!(
            restarted
                .prepare_memo(&memo, external_claim_message_id(&key))
                .await
                .unwrap(),
            plan
        );
        assert_eq!(
            restarted
                .claim_external_memo(
                    TransferMemo {
                        entries: memo.entries.clone(),
                        total_value: 20
                    },
                    external_claim_message_id(&key),
                )
                .await
                .unwrap(),
            20
        );
        rig.backend.state.lock().clear();
        assert_eq!(
            restarted
                .claim_external_memo(memo, external_claim_message_id(&key))
                .await
                .unwrap(),
            20
        );
        assert_eq!(rig.backend.transfers.lock().len(), 1);
    }

    #[tokio::test]
    async fn absent_source_and_destination_without_receipt_never_clear() {
        let source = expanded_secret(9);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let key = memo.identifier();
        let rig = rig([(
            public(&source),
            OnChainCoin {
                exponent: 1,
                age: 2,
            },
        )]);
        rig.service
            .prepare_memo(&memo, external_claim_message_id(&key))
            .await
            .unwrap();
        rig.backend.state.lock().clear();
        assert!(
            rig.service
                .claim_external_memo(memo, external_claim_message_id(&key))
                .await
                .is_err()
        );
        let plan = rig.plans.plan(&key).await.unwrap().unwrap();
        assert_ne!(plan.status, ClaimPlanStatus::Finished);
        assert_eq!(plan.claimed_amount, None);
        assert!(rig.backend.transfers.lock().is_empty());
        assert!(rig.coins.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn partial_finalized_prefix_survives_spent_destination_and_retry() {
        let first = expanded_secret(14);
        let second = expanded_secret(15);
        let memo = TransferMemo {
            entries: vec![first.clone(), second.clone()],
            total_value: 60,
        };
        let key = memo.identifier();
        let rig = rig([
            (
                public(&first),
                OnChainCoin {
                    exponent: 1,
                    age: 2,
                },
            ),
            (
                public(&second),
                OnChainCoin {
                    exponent: 2,
                    age: 2,
                },
            ),
        ]);
        let plan = rig
            .service
            .prepare_memo(&memo, external_claim_message_id(&key))
            .await
            .unwrap();
        let destination = CoinKeypairFactory::new(&[0x44; 16])
            .public_key(plan.entries[0].derivation_index)
            .unwrap();
        {
            let mut state = rig.backend.state.lock();
            state.remove(&public(&first));
            state.remove(&public(&second));
            state.insert(
                destination,
                OnChainCoin {
                    exponent: 1,
                    age: 0,
                },
            );
        }
        assert!(
            rig.service
                .claim_external_memo(
                    TransferMemo {
                        entries: memo.entries.clone(),
                        total_value: 60
                    },
                    external_claim_message_id(&key),
                )
                .await
                .is_err()
        );
        assert_eq!(
            rig.plans.plan(&key).await.unwrap().unwrap().claimed_amount,
            Some(20)
        );
        {
            let mut state = rig.backend.state.lock();
            state.remove(&destination);
            state.insert(
                public(&second),
                OnChainCoin {
                    exponent: 2,
                    age: 2,
                },
            );
        }
        let restarted = ExternalSecretClaimService::new(
            &[0x44; 16],
            Arc::new(CoinAllocator::new(Arc::new(
                InMemoryCoinageIndexStore::default(),
            ))),
            rig.coins.clone(),
            rig.plans.clone(),
            rig.backend.clone(),
        );
        assert_eq!(
            restarted
                .claim_external_memo(memo, external_claim_message_id(&key))
                .await
                .unwrap(),
            60
        );
        assert_eq!(
            rig.backend
                .transfers
                .lock()
                .iter()
                .map(|transfer| transfer.source)
                .collect::<Vec<_>>(),
            vec![public(&second)]
        );
        assert_eq!(
            rig.plans.plan(&key).await.unwrap().unwrap().status,
            ClaimPlanStatus::Finished
        );
    }

    /// At one finalized snapshot, both keys present is a destination collision,
    /// not proof this memo's source was transferred.
    #[tokio::test]
    async fn landed_destination_with_unconsumed_source_never_clears() {
        let source = expanded_secret(10);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let memo_key = memo.identifier();
        let recipient = CoinKeypairFactory::new(&[0x44; 16]).public_key(0).unwrap();
        let rig = rig([
            (
                public(&source),
                OnChainCoin {
                    exponent: 1,
                    age: 2,
                },
            ),
            (
                recipient,
                OnChainCoin {
                    exponent: 1,
                    age: 5,
                },
            ),
        ]);
        rig.plans.0.lock().insert(
            memo_key,
            ClaimPlan {
                memo_key,
                message_id: Some("chat-message-1".into()),
                entries: vec![CodableClaimPlanEntry {
                    entry_index: 0,
                    exponent: 1,
                    derivation_index: 0,
                }],
                outgoing_public_keys: Vec::new(),
                detection_anchor: None,
                status: ClaimPlanStatus::Processing,
                claimed_amount: None,
                total_value: 20,
            },
        );

        assert!(
            rig.service
                .claim_external_memo(memo, "chat-message-1".into())
                .await
                .is_err()
        );
        assert!(rig.backend.transfers.lock().is_empty());
        assert!(rig.coins.list().await.unwrap().is_empty());
        assert_eq!(
            rig.plans
                .plan(&memo_key)
                .await
                .unwrap()
                .unwrap()
                .claimed_amount,
            None
        );
    }

    /// A plan persisted before chat claims carried chat message ids keeps
    /// working when the claim now arrives under the chat row's id.
    #[tokio::test]
    async fn legacy_external_id_plan_accepts_the_chat_message_id() {
        let source = expanded_secret(11);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let memo_key = memo.identifier();
        let rig = rig([]);
        let recipient = CoinKeypairFactory::new(&[0x44; 16]).public_key(0).unwrap();
        rig.backend.state.lock().insert(
            recipient,
            OnChainCoin {
                exponent: 1,
                age: 3,
            },
        );
        rig.plans.0.lock().insert(
            memo_key,
            ClaimPlan {
                memo_key,
                message_id: Some(external_claim_message_id(&memo_key)),
                entries: vec![CodableClaimPlanEntry {
                    entry_index: 0,
                    exponent: 1,
                    derivation_index: 0,
                }],
                outgoing_public_keys: Vec::new(),
                detection_anchor: None,
                status: ClaimPlanStatus::Error,
                claimed_amount: None,
                total_value: 20,
            },
        );

        let claimed = rig
            .service
            .claim_external_memo(memo, "chat-message-1".into())
            .await
            .unwrap();
        assert_eq!(claimed, 20);
        assert_eq!(
            rig.plans.0.lock().get(&memo_key).unwrap().status,
            ClaimPlanStatus::Finished,
            "a persisted error heals once the destination is proven"
        );
    }

    #[tokio::test]
    async fn source_and_recipient_presence_collision_fails_closed() {
        let source = expanded_secret(7);
        let memo = TransferMemo {
            entries: vec![source.clone()],
            total_value: 20,
        };
        let memo_key = memo.identifier();
        let recipient = CoinKeypairFactory::new(&[0x44; 16]).public_key(0).unwrap();
        let rig = rig([
            (
                public(&source),
                OnChainCoin {
                    exponent: 1,
                    age: 0,
                },
            ),
            (
                recipient,
                OnChainCoin {
                    exponent: 1,
                    age: 0,
                },
            ),
        ]);

        let error = rig
            .service
            .claim_external_memo(memo, external_claim_message_id(&memo_key))
            .await
            .unwrap_err();
        assert!(error.contains("destination already exists"));
        assert!(rig.backend.transfers.lock().is_empty());
        assert!(rig.plans.0.lock().is_empty());
    }

    #[tokio::test]
    async fn spent_recovery_restores_max_age_and_reuses_the_claim_transfer_lane() {
        let factory = CoinKeypairFactory::new(&[0x44; 16]);
        let max_age = Coin {
            exponent: 1,
            derivation_index: 3,
            age: Some(1),
            state: CoinState::Spent,
        };
        let transferable = Coin {
            exponent: 2,
            derivation_index: 4,
            age: Some(1),
            state: CoinState::Spent,
        };
        let absent = Coin {
            exponent: 0,
            derivation_index: 5,
            age: Some(1),
            state: CoinState::Spent,
        };
        let rig = rig([
            (
                factory.public_key(max_age.derivation_index).unwrap(),
                OnChainCoin {
                    exponent: max_age.exponent,
                    age: COIN_MAX_AGE,
                },
            ),
            (
                factory.public_key(transferable.derivation_index).unwrap(),
                OnChainCoin {
                    exponent: transferable.exponent,
                    age: COIN_MAX_AGE - 1,
                },
            ),
        ]);
        for coin in [&max_age, &transferable, &absent] {
            rig.coins.upsert(coin).await.unwrap();
        }
        let recovery = SpentCoinTransferRecoveryService::new(
            &[0x44; 16],
            Arc::clone(&rig.coins) as Arc<_>,
            Arc::clone(&rig.backend) as Arc<_>,
            Arc::clone(&rig.service),
        );

        let report = recovery
            .recover(vec![max_age.clone(), transferable.clone(), absent])
            .await
            .unwrap();
        assert_eq!(
            report,
            SpentCoinTransferRecoveryReport {
                recovered_count: 2,
                recovered_planks: 60,
                restored_max_age_count: 1,
                transferred_count: 1,
            }
        );
        assert_eq!(rig.backend.transfers.lock().len(), 1);
        let coins = rig.coins.list().await.unwrap();
        assert_eq!(
            coins
                .iter()
                .find(|coin| coin.derivation_index == max_age.derivation_index)
                .unwrap()
                .state,
            CoinState::Available
        );
        assert_eq!(
            coins
                .iter()
                .find(|coin| coin.derivation_index == transferable.derivation_index)
                .unwrap()
                .state,
            CoinState::Spent
        );
        assert!(
            coins.iter().any(|coin| coin.derivation_index == 0
                && coin.exponent == transferable.exponent
                && coin.state == CoinState::Available),
            "the younger source lands in the allocator's fresh destination"
        );
    }

    #[tokio::test]
    async fn spent_recovery_rejects_non_spent_inputs_before_querying() {
        let rig = rig([]);
        let recovery = SpentCoinTransferRecoveryService::new(
            &[0x44; 16],
            Arc::clone(&rig.coins) as Arc<_>,
            Arc::clone(&rig.backend) as Arc<_>,
            Arc::clone(&rig.service),
        );
        let error = recovery
            .recover(vec![Coin {
                exponent: 0,
                derivation_index: 0,
                age: Some(0),
                state: CoinState::Available,
            }])
            .await
            .unwrap_err();
        assert!(error.contains("non-spent"));
        assert!(rig.backend.transfers.lock().is_empty());
    }
}

/// Secret-preserving claim operation; implementations do not expose memo bytes
/// through a product or transport API.
#[async_trait]
pub trait ExternalMemoClaiming: Send + Sync {
    /// Execute or resume a durable memo-keyed claim into this wallet.
    async fn claim_external_memo(
        &self,
        memo: TransferMemo,
        message_id: String,
    ) -> Result<u128, String>;
}

/// Recover locally spent coins whose finalized on-chain value remains owned.
#[async_trait]
pub trait SpentCoinsRecovering: Send + Sync {
    /// Reconcile and recover the supplied spent inventory.
    async fn recover_spent_coins(&self, spent: Vec<Coin>) -> Result<u128, String>;
}
