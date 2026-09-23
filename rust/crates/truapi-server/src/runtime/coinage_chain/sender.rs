// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer core/crates/brevity-ffi/src/coinage_sender.rs.
// Copyright the Brevity contributors. See truapi-coinage/NOTICE and LICENSE.

use super::transaction::{Snapshot, constant, decode_exact, denomination_value, storage_key};
use super::{HostCoinageChain, crypto::HostPersonProof, rpc};
use parity_scale_codec::Encode;
use scale_decode::DecodeAsType;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use subxt_rpcs::RpcClient;
use truapi_coinage::{
    AliasState, BandersnatchRingProofProvider, CheckpointBlock, Coin, PersonOriginKind,
    PreparedUnloadGroup, RegularTransferSubmitter, ResolvedUnloadToken, RingProofParams,
    RingProofProvider, SplitTransferSubmission, UnloadGroupDraft, UnloadProofRequest,
};
use truapi_platform::async_trait;

const PEOPLE: &[u8; 32] = b"pop:polkadot.network/people     ";
const PEOPLE_LITE: &[u8; 32] = b"pop:polkadot.network/people-lite";

type SplitDestinations = (
    Vec<truapi_coinage::pallet::SplitDestination>,
    Vec<[u8; 32]>,
    u128,
);

impl HostCoinageChain {
    async fn person_proof(
        &self,
        rpc: &RpcClient,
        snapshot: &Snapshot,
    ) -> Result<Arc<HostPersonProof>, String> {
        let key = storage_key(&snapshot.storage, "NetworkSuffix", "NetworkSuffix", &[])?;
        let value = rpc::query(rpc, &[key], snapshot.at)
            .await?
            .pop()
            .flatten()
            .ok_or("Coinage network suffix is missing")?;
        let suffix: Vec<u8> = decode_exact(&value)?;
        let suffix =
            core::str::from_utf8(&suffix).map_err(|_| "Coinage network suffix is invalid")?;
        self.ensure_session()?;
        Ok(Arc::new(HostPersonProof::new(
            &self.inner.entropy,
            suffix,
            self.inner.session_valid.clone(),
        )?))
    }

    async fn resolve_person(
        &self,
        rpc: &RpcClient,
        snapshot: &Snapshot,
        proof: &HostPersonProof,
    ) -> Result<(PersonOriginKind, RingProofParams), String> {
        for (origin, collection) in [
            (PersonOriginKind::Full, PEOPLE),
            (PersonOriginKind::Lite, PEOPLE_LITE),
        ] {
            let member = proof.member(origin)?;
            let key = storage_key(
                &snapshot.storage,
                "Members",
                "Members",
                &[collection.encode(), member.encode()],
            )?;
            let Some(value) = rpc::query(rpc, &[key], snapshot.at).await?.pop().flatten() else {
                continue;
            };
            let position: truapi_coinage::members::RingPosition = decode_exact(&value)?;
            let truapi_coinage::members::RingPosition::Included { ring_index, .. } = position
            else {
                continue;
            };
            if let Some(ring) =
                ring_snapshot(rpc, snapshot, collection, ring_index, &[member]).await?
            {
                return Ok((origin, ring));
            }
        }
        Err("no finalized full or lite People origin is proof-ready".into())
    }

    async fn tokens(
        &self,
        rpc: &RpcClient,
        snapshot: &Snapshot,
        proof: &HostPersonProof,
        origin: PersonOriginKind,
        count: usize,
    ) -> Result<Vec<ResolvedUnloadToken>, String> {
        let duration: u32 = constant(
            &snapshot.metadata,
            "Coinage",
            "UnloadTokenTimePeriodPeopleLitePeople",
        )?;
        // Eligibility is price-dependent and differs for full and lite people.
        // Read it at the same finalized snapshot as the token-consumption checks.
        let (full, lite) = rpc::free_unload_token_limits(rpc, snapshot).await?;
        let maximum = match origin {
            PersonOriginKind::Full => full,
            PersonOriginKind::Lite => lite,
        };
        if duration == 0 || maximum > 65536 {
            return Err("invalid Coinage free-token bounds".into());
        }
        if maximum == 0 {
            return Err("no free Coinage unload tokens available for this origin".into());
        }
        // Chain time, never an ambient system clock, defines the eligibility period.
        let key = storage_key(&snapshot.storage, "Timestamp", "Now", &[])?;
        let now: u64 = decode_exact(
            &rpc::query(rpc, &[key], snapshot.at)
                .await?
                .pop()
                .flatten()
                .ok_or("finalized timestamp missing")?,
        )?;
        let now = now / 1000;
        let current = u32::try_from(now / u64::from(duration))
            .map_err(|_| "Coinage token period out of range")?;
        let old = u32::try_from(now.saturating_sub(3600) / u64::from(duration))
            .map_err(|_| "Coinage token period out of range")?;
        let periods = if old == current {
            vec![current]
        } else {
            vec![old, current]
        };
        let mut available = Vec::with_capacity(count);
        for period in periods {
            // Bound each query and avoid deriving unused token aliases once enough are found.
            for first in (0..maximum).step_by(128) {
                let mut candidates = Vec::new();
                let mut keys = Vec::new();
                for counter in first..maximum.min(first.saturating_add(128)) {
                    self.ensure_session()?;
                    let token = ResolvedUnloadToken { period, counter };
                    let alias = proof.alias(origin, &token.context())?;
                    keys.push(storage_key(
                        &snapshot.storage,
                        "Coinage",
                        "ConsumedFreeUnloadTokens",
                        &[period.encode(), alias.encode()],
                    )?);
                    candidates.push(token);
                }
                for (token, value) in candidates
                    .into_iter()
                    .zip(rpc::query(rpc, &keys, snapshot.at).await?)
                {
                    if value.is_none() {
                        available.push(token);
                    }
                    if available.len() == count {
                        return Ok(available);
                    }
                }
            }
        }
        Err("insufficient finalized free Coinage unload tokens".into())
    }

    async fn prepare_groups_at(
        &self,
        rpc: &RpcClient,
        snapshot: &Snapshot,
        groups: &[UnloadGroupDraft],
    ) -> Result<Vec<PreparedUnloadGroup>, String> {
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        self.bind_denominations(snapshot)?;
        let maximum: u32 = constant(&snapshot.metadata, "Coinage", "MaxConsolidation")?;
        let person = self.person_proof(rpc, snapshot).await?;
        let (origin, people_ring) = self.resolve_person(rpc, snapshot, &person).await?;
        let tokens = self
            .tokens(rpc, snapshot, &person, origin, groups.len())
            .await?;
        self.ensure_session()?;
        let vouchers = &self.inner.vouchers;
        let mut prepared = Vec::with_capacity(groups.len());
        for (group, token) in groups.iter().zip(tokens) {
            if group.vouchers.is_empty() || group.vouchers.len() > maximum as usize {
                return Err("voucher group violates runtime consolidation bounds".into());
            }
            let mut unique = HashSet::new();
            let required = group
                .vouchers
                .iter()
                .map(|voucher| {
                    if voucher.exponent != group.recycler.exponent
                        || !unique.insert(voucher.derivation_index)
                    {
                        return Err("invalid Coinage voucher group".into());
                    }
                    self.ensure_session()?;
                    vouchers.public_key(voucher.derivation_index, self.inner.crypto.as_ref())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let collection = snapshot.storage.collection(group.recycler.exponent);
            let recycler_ring =
                ring_snapshot(rpc, snapshot, &collection, group.recycler.index, &required)
                    .await?
                    .ok_or("voucher recycler ring is not proof-ready")?;
            prepared.push(PreparedUnloadGroup {
                draft: group.clone(),
                readiness_block_hash: snapshot.at,
                recycler_revision: recycler_ring.ring_revision,
                origin: truapi_coinage::UnloadOriginPreparation {
                    recycler_ring,
                    person_origin: origin,
                    people_ring: people_ring.clone(),
                    token,
                },
            });
        }
        Ok(prepared)
    }

    async fn checkpoint(&self, id: &str, snapshot: &Snapshot) -> Result<(), String> {
        self.ensure_session()?;
        self.inner
            .wal
            .update_checkpoint(
                id,
                CheckpointBlock::Known {
                    number: snapshot.number,
                    hash: snapshot.at,
                },
            )
            .await
            .map_err(|_| "Coinage durable checkpoint failed")?;
        self.ensure_session()
    }

    fn destinations(
        &self,
        coins: &[&Coin],
        snapshot: &Snapshot,
    ) -> Result<SplitDestinations, String> {
        if coins.is_empty() {
            return Err("Coinage transfer has no destinations".into());
        }
        self.ensure_session()?;
        let factory = &self.inner.coins;
        let context = snapshot.denomination_context()?;
        let mut unique = HashSet::new();
        let mut grouped = BTreeMap::<i16, Vec<[u8; 32]>>::new();
        let mut owners = Vec::with_capacity(coins.len());
        let mut total = 0u128;
        for coin in coins {
            if !unique.insert(coin.derivation_index) {
                return Err("duplicate Coinage destination".into());
            }
            self.ensure_session()?;
            let owner = factory.public_key(coin.derivation_index)?;
            total = total
                .checked_add(denomination_value(&context, coin.exponent)?)
                .ok_or("Coinage output amount overflow")?;
            owners.push(owner);
            grouped.entry(coin.exponent).or_default().push(owner);
        }
        Ok((
            grouped
                .into_iter()
                .map(
                    |(exponent, accounts)| truapi_coinage::pallet::SplitDestination {
                        exponent,
                        accounts,
                    },
                )
                .collect(),
            owners,
            total,
        ))
    }

    async fn verify_outputs(
        &self,
        rpc: &RpcClient,
        coins: &[&Coin],
        owners: &[[u8; 32]],
        snapshot: &Snapshot,
    ) -> Result<(), String> {
        let outputs = Self::coins_at(rpc, owners, snapshot).await?;
        if outputs.len() != coins.len()
            || outputs
                .iter()
                .zip(coins)
                .any(|(output, coin)| output.is_none_or(|output| output.exponent != coin.exponent))
        {
            return Err("finalized Coinage outputs do not match the transfer".into());
        }
        Ok(())
    }
}

#[async_trait]
impl RegularTransferSubmitter for HostCoinageChain {
    async fn prepare_unload_groups(
        &self,
        groups: &[UnloadGroupDraft],
    ) -> Result<Vec<PreparedUnloadGroup>, String> {
        self.ensure_session()?;
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let result = self.prepare_groups_at(&rpc, &snapshot, groups).await?;
        self.ensure_session()?;
        Ok(result)
    }

    async fn submit_split(&self, submission: &SplitTransferSubmission) -> Result<(), String> {
        let _gate = self.inner.submission.lock().await;
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        let context = self.bind_denominations(&snapshot)?;
        let coins: Vec<_> = submission
            .recipient_coins
            .iter()
            .chain(&submission.change_coins)
            .collect();
        let (destinations, owners, total) = self.destinations(&coins, &snapshot)?;
        if total != denomination_value(&context, submission.overflow_coin.exponent)? {
            return Err("Coinage split does not conserve its input denomination".into());
        }
        self.ensure_session()?;
        let factory = &self.inner.coins;
        let keypair = factory.keypair(submission.overflow_coin.derivation_index)?;
        let source = keypair.public.to_bytes();
        if owners.contains(&source) {
            return Err("Coinage split reuses its source".into());
        }
        let mut check = vec![source];
        check.extend_from_slice(&owners);
        let before = Self::coins_at(&rpc, &check, &snapshot).await?;
        if before[0].is_none_or(|coin| coin.exponent != submission.overflow_coin.exponent)
            || before[1..].iter().any(Option::is_some)
        {
            return Err("Coinage split input or outputs changed".into());
        }
        let [pallet, index] = snapshot
            .metadata
            .call_indices("Coinage", "split")
            .map_err(|_| "Coinage split call unavailable")?;
        let call = truapi_coinage::pallet::split_call(pallet, index, &destinations)
            .map_err(|_| "invalid Coinage split call")?;
        let nonce = self.nonce(&rpc, &source, &snapshot).await?;
        self.ensure_session()?;
        let extrinsic = snapshot.signed(&keypair, &call, nonce)?;
        self.checkpoint(&submission.wal_entry_id, &snapshot).await?;
        let finalized = self.broadcast(&rpc, &extrinsic, &snapshot).await?;
        let finalized_snapshot = self.snapshot(&rpc, finalized).await?;
        if Self::coins_at(&rpc, &[source], &finalized_snapshot).await?[0].is_some() {
            return Err("finalized Coinage split left its source unconsumed".into());
        }
        self.verify_outputs(&rpc, &coins, &owners, &finalized_snapshot)
            .await
    }

    async fn submit_unload_group(&self, submission: &PreparedUnloadGroup) -> Result<(), String> {
        let _gate = self.inner.submission.lock().await;
        let rpc = self.connect().await?;
        let at = rpc::finalized(&rpc).await?;
        let snapshot = self.snapshot(&rpc, at).await?;
        // Re-resolve every ring/revision/token at the signing snapshot. A prepared
        // preview is not authority to sign stale roots or a now-consumed token.
        let fresh = self
            .prepare_groups_at(&rpc, &snapshot, core::slice::from_ref(&submission.draft))
            .await?
            .pop()
            .ok_or("missing unload preparation")?;
        let coins: Vec<_> = fresh
            .draft
            .recipient_coins
            .iter()
            .chain(&fresh.draft.change_coins)
            .collect();
        let (destinations, owners, total) = self.destinations(&coins, &snapshot)?;
        let expected = denomination_value(
            &snapshot.denomination_context()?,
            fresh.draft.recycler.exponent,
        )?
        .checked_mul(fresh.draft.vouchers.len() as u128)
        .ok_or("Coinage input amount overflow")?;
        if expected != total {
            return Err("Coinage unload does not conserve input denominations".into());
        }
        if Self::coins_at(&rpc, &owners, &snapshot)
            .await?
            .iter()
            .any(Option::is_some)
        {
            return Err("Coinage unload destination already exists".into());
        }
        let person = self.person_proof(&rpc, &snapshot).await?;
        self.ensure_session()?;
        let provider = BandersnatchRingProofProvider::new(
            &self.inner.entropy,
            self.inner.crypto.clone(),
            person,
        );
        let indices: Vec<_> = fresh
            .draft
            .vouchers
            .iter()
            .map(|voucher| voucher.derivation_index)
            .collect();
        let aliases = provider
            .unload_aliases(&indices)
            .map_err(|_| "Coinage unload alias derivation failed")?;
        let exponent = i8::try_from(fresh.draft.recycler.exponent)
            .map_err(|_| "Coinage exponent out of range")?;
        let alias_keys = |snapshot: &Snapshot| {
            aliases
                .iter()
                .map(|alias| {
                    snapshot.storage.coinage_key(
                        &truapi_coinage::CoinageStorageKey::RecyclerAlias {
                            exponent,
                            ring_index: fresh.draft.recycler.index,
                            alias: *alias,
                        },
                    )
                })
                .collect::<Result<Vec<_>, String>>()
        };
        if rpc::query(&rpc, &alias_keys(&snapshot)?, snapshot.at)
            .await?
            .iter()
            .any(Option::is_some)
        {
            return Err("Coinage unload alias is consumed or locked".into());
        }
        let [pallet, index] = snapshot
            .metadata
            .call_indices("Coinage", "unload_recycler_into_coins")
            .map_err(|_| "Coinage unload call unavailable")?;
        let call = truapi_coinage::pallet::unload_recycler_into_coins_call(
            pallet,
            index,
            snapshot.storage.instance_id,
            &aliases,
            exponent,
            fresh.draft.recycler.index,
            fresh.recycler_revision,
            &destinations,
            0,
        )
        .map_err(|_| "invalid Coinage unload call")?;
        let extensions = snapshot.extensions(0)?;
        let implication = snapshot.implication(&call, &extensions)?;
        self.ensure_session()?;
        let proof = provider
            .unload_proof(&UnloadProofRequest {
                recycler: fresh.draft.recycler,
                voucher_derivation_indices: indices,
                recycler_ring: fresh.origin.recycler_ring,
                person_origin: fresh.origin.person_origin,
                people_ring: fresh.origin.people_ring,
                token: fresh.origin.token,
                inherited_implication: implication,
            })
            .map_err(|_| "Coinage unload proof failed")?;
        if proof.aliases != aliases {
            return Err("Coinage proof aliases changed".into());
        }
        self.ensure_session()?;
        let extrinsic = snapshot.unsigned(&call, extensions, &proof)?;
        self.checkpoint(&fresh.draft.wal_entry_id, &snapshot)
            .await?;
        let finalized = self.broadcast(&rpc, &extrinsic, &snapshot).await?;
        let finalized_snapshot = self.snapshot(&rpc, finalized).await?;
        let values = rpc::query(&rpc, &alias_keys(&finalized_snapshot)?, finalized).await?;
        for value in values {
            if value
                .as_deref()
                .map(decode_exact::<AliasState>)
                .transpose()?
                != Some(AliasState::Unloaded)
            {
                return Err("finalized Coinage unload did not consume every alias".into());
            }
        }
        self.verify_outputs(&rpc, &coins, &owners, &finalized_snapshot)
            .await
    }
}

#[derive(DecodeAsType)]
struct CollectionInfo {
    ring_size: RingExponent,
}
#[derive(DecodeAsType)]
enum RingExponent {
    R2e9,
    R2e10,
    R2e14,
}

async fn ring_snapshot(
    rpc: &RpcClient,
    snapshot: &Snapshot,
    collection: &[u8; 32],
    index: u32,
    required: &[[u8; 32]],
) -> Result<Option<RingProofParams>, String> {
    let keys = [
        storage_key(
            &snapshot.storage,
            "Members",
            "Collections",
            &[collection.encode()],
        )?,
        storage_key(
            &snapshot.storage,
            "Members",
            "RingKeysStatus",
            &[collection.encode(), index.encode()],
        )?,
        storage_key(
            &snapshot.storage,
            "Members",
            "Root",
            &[collection.encode(), index.encode()],
        )?,
    ];
    let values = rpc::query(rpc, &keys, snapshot.at).await?;
    let [Some(collection_info), Some(status), Some(root)] = values.as_slice() else {
        return Ok(None);
    };
    let type_id = snapshot
        .metadata
        .storage_value_type("Members", "Collections")
        .ok_or("Members collection type unavailable")?;
    let mut input = collection_info.as_slice();
    let info = CollectionInfo::decode_as_type(&mut input, type_id, snapshot.metadata.registry())
        .map_err(|_| "invalid Members collection")?;
    if !input.is_empty() {
        return Err("Members collection has trailing bytes".into());
    }
    let exponent = match info.ring_size {
        RingExponent::R2e9 => 9,
        RingExponent::R2e10 => 10,
        RingExponent::R2e14 => 14,
    };
    let status: truapi_coinage::members::RingStatus = decode_exact(status)?;
    let root: truapi_coinage::members::RingRoot = decode_exact(root)?;
    if status.included == 0 || status.included > status.total || status.total > (1 << exponent) {
        return Ok(None);
    }
    let prefix = storage_key(
        &snapshot.storage,
        "Members",
        "RingKeys",
        &[collection.encode(), index.encode()],
    )?;
    let mut pages = rpc::keys(rpc, &prefix, snapshot.at).await?;
    pages.sort_by_key(|key| {
        key.get(key.len().saturating_sub(4)..)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
    });
    for (page, key) in pages.iter().enumerate() {
        let expected = storage_key(
            &snapshot.storage,
            "Members",
            "RingKeys",
            &[collection.encode(), index.encode(), (page as u32).encode()],
        )?;
        if key != &expected {
            return Err("Members ring page sequence is incomplete".into());
        }
    }
    let mut members = Vec::with_capacity(status.included as usize);
    for value in rpc::query(rpc, &pages, snapshot.at).await? {
        let value = value.ok_or("Members ring page missing")?;
        let page: Vec<[u8; 32]> = decode_exact(&value)?;
        members.extend(
            page.into_iter()
                .take((status.included as usize).saturating_sub(members.len())),
        );
        if members.len() == status.included as usize {
            break;
        }
    }
    if members.len() != status.included as usize
        || required.iter().any(|member| !members.contains(member))
    {
        return Ok(None);
    }
    if members.iter().collect::<HashSet<_>>().len() != members.len() {
        return Err("Members ring contains duplicate keys".into());
    }
    Ok(Some(RingProofParams {
        ring_exponent: exponent,
        ring_index: index,
        ring_revision: root.revision,
        ring_members: members,
    }))
}
