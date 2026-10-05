use super::{announce_for_session, establish_pairing_for_session, notify_pairing_failed, PairingProposal};
use crate::runtime::signing_host::allowance_renewal::{self, StatementRenewalTarget};
use crate::runtime::authority::{AccountHolder, AuthoritySession};
use crate::runtime::{RuntimeServices, WalletAccountHolder};
use std::sync::Arc;
use crate::platform::CoreStorageKey;
use parity_scale_codec::{Decode, DecodeAll, Encode};

/// Provision both statement accounts and persist a pairing under one wallet activation.
pub async fn establish_pairing_with_allowances(
    signing_host: &WalletAccountHolder,
    services: &Arc<RuntimeServices>,
    activation: &AuthoritySession,
    deeplink: &str,
) -> Result<(), String> {
    signing_host.require_current_session(activation).map_err(|error| error.to_string())?;
    cleanup_pending_pairings(signing_host, services, activation.public_key, Some(activation)).await?;
    let proposal = PairingProposal::from_deeplink(deeplink)?;
    let account_id = proposal.peer.statement_account_id;
    let tracked = allowance_renewal::list(signing_host).await?;
    let existing_label = tracked.iter().find_map(|entry| match &entry.target {
        StatementRenewalTarget::Account { account_id: stored, label } if *stored == account_id && entry.owner == Some(activation.public_key) => Some(label.clone()),
        _ => None,
    });
    allowance_renewal::track_for_session(signing_host, activation, vec![StatementRenewalTarget::WalletSso]).await?;
    let report = allowance_renewal::renew_for_session(services, signing_host, activation).await?;
    require_pairing_allowance(&report, "wallet-sso")?;
    let announced = announce_for_session(services, signing_host, deeplink, activation).await?;
    let label = existing_label.clone().unwrap_or_else(|| format!("device:{}", hex::encode(account_id)));
    let result = async {
        if existing_label.is_none() {
            remember_pending_pairing(signing_host, services, activation, proposal.peer).await?;
        }
        allowance_renewal::track_for_session(signing_host, activation, vec![StatementRenewalTarget::Account { account_id, label: label.clone() }]).await?;
        let report = allowance_renewal::renew_for_session(services, signing_host, activation).await?;
        require_pairing_allowance(&report, &label)?;
        establish_pairing_for_session(services, signing_host, deeplink, activation).await?;
        signing_host.require_current_session(activation).map(drop).map_err(|error| error.to_string())
    }.await;
    if let Err(mut reason) = result {
        if let Err(notice) = notify_pairing_failed(services.clone(), &announced, reason.clone()).await {
            reason.push_str(&format!("; pairing failure notice failed: {notice}"));
        }
        if let Err(cleanup) = cleanup_pending_pairings(signing_host, services, activation.public_key, Some(activation)).await {
            reason.push_str(&format!("; renewal cleanup deferred: {cleanup}"));
        }
        return Err(reason)
    }
    let _persistence = signing_host.persistence.lock().await;
    signing_host.require_current_session(activation).map_err(|error| error.to_string())?;
    services.platform.clear_core_storage(CoreStorageKey::PendingPairingCleanup { root_public_key: activation.public_key }).await.map_err(|error| error.reason)
}

fn require_pairing_allowance(report: &crate::runtime::statement_allowance::renewal::StatementRenewalReport, label: &str) -> Result<(), String> {
    use crate::runtime::statement_allowance::renewal::TargetRenewalStatus;
    let status = report.outcomes.iter().find(|outcome| outcome.label == label).map(|outcome| &outcome.status);
    match status {
        Some(TargetRenewalStatus::Registered { .. } | TargetRenewalStatus::AlreadyAllocated { .. }) => Ok(()),
        Some(TargetRenewalStatus::Failed { reason }) => Err(format!("{label} allowance failed: {reason}")),
        Some(TargetRenewalStatus::SkippedExhausted) => Err(format!("no statement slots available for {label}")),
        None => Err(format!("no allowance result for {label}")),
    }
}


#[derive(Encode, Decode)]
struct PendingPairingCleanup {
    peer_statement: [u8; 32],
    peer_encryption: [u8; 32],
    preserve_slots: bool,
}

async fn pending_pairings(services: &RuntimeServices, owner: [u8; 32]) -> Result<Vec<PendingPairingCleanup>, String> {
    let value = services.platform.read_core_storage(CoreStorageKey::PendingPairingCleanup { root_public_key: owner }).await.map_err(|error| error.reason)?;
    match value {
        None => Ok(Vec::new()),
        Some(value) => Vec::decode_all(&mut value.as_slice()).map_err(|error| format!("invalid pending pairing cleanup: {error}")),
    }
}

async fn remember_pending_pairing(
    holder: &WalletAccountHolder,
    services: &RuntimeServices,
    activation: &AuthoritySession,
    peer: super::PairedSsoPeer,
) -> Result<(), String> {
    let _persistence = holder.persistence.lock().await;
    holder.require_current_session(activation).map_err(|error| error.to_string())?;
    let mut pending = pending_pairings(services, activation.public_key).await?;
    #[cfg(not(target_arch = "wasm32"))]
    let preserve_slots = match services.runtime_store() {
        Some(store) => store.statement_slots().await.map_err(|error| error.to_string())?.iter().any(|slot| slot.account == peer.statement_account_id),
        None => false,
    };
    #[cfg(target_arch = "wasm32")]
    let preserve_slots = false;
    pending.push(PendingPairingCleanup { peer_statement: peer.statement_account_id, peer_encryption: peer.encryption_public_key, preserve_slots });
    holder.require_current_session(activation).map_err(|error| error.to_string())?;
    services.platform.write_core_storage(CoreStorageKey::PendingPairingCleanup { root_public_key: activation.public_key }, pending.encode()).await.map_err(|error| error.reason)
}

/// Complete interrupted pairing cleanup before unlocking or closing this owner's store.
pub async fn cleanup_pending_pairings(
    holder: &WalletAccountHolder,
    services: &RuntimeServices,
    owner: [u8; 32],
    activation: Option<&AuthoritySession>,
) -> Result<(), String> {
    let _persistence = holder.persistence.lock().await;
    match activation {
        Some(activation) => { holder.require_current_session(activation).map_err(|error| error.to_string())?; }
        None if holder.current_session().is_some() => return Err("pairing cleanup requires a locked wallet".to_string()),
        None => {}
    }
    let pending = pending_pairings(services, owner).await?;
    if pending.is_empty() { return Ok(()) }
    #[cfg(not(target_arch = "wasm32"))]
    let store = services.runtime_store();
    #[cfg(not(target_arch = "wasm32"))]
    if store.as_ref().is_some_and(|store| store.owner() != owner) { return Err("pairing cleanup store belongs to another wallet".to_string()) }
    for cleanup in pending {
        #[cfg(not(target_arch = "wasm32"))]
        let completed = match &store {
            Some(store) => store.paired_hosts().await.map_err(|error| error.to_string())?.iter().any(|peer| peer.peer_statement == cleanup.peer_statement && peer.status == "paired"),
            None => false,
        };
        #[cfg(target_arch = "wasm32")]
        let completed = false;
        if !completed {
            allowance_renewal::untrack_account(services.platform.as_ref(), holder.renewal.ledger_lock(), owner, &cleanup.peer_statement).await?;
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(store) = &store {
                if !cleanup.preserve_slots { store.remove_statement_allowance(cleanup.peer_statement).await.map_err(|error| error.to_string())?; }
                store.remove_paired_host(cleanup.peer_statement, cleanup.peer_encryption).await.map_err(|error| error.to_string())?;
            }
        }
    }
    services.platform.clear_core_storage(CoreStorageKey::PendingPairingCleanup { root_public_key: owner }).await.map_err(|error| error.reason)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::host_core::SigningHostRuntime;
    use crate::latest::GenericError;
    use crate::platform::{HostInfo, PlatformInfo, SigningHostConfig, WalletSecretProvider};
    use crate::runtime::signing_host::LocalActivation;
    use crate::store::{AllowanceRecord, Db, PairedHostRecord, RuntimeStore, StatementSlotRecord, account_core_db_config};
    use crate::test_support::{StubPlatform, test_spawner};

    struct WalletSecrets;

    #[async_trait::async_trait]
    impl WalletSecretProvider for WalletSecrets {
        async fn read_wallet_root_entropy(&self, _: &str) -> Result<Vec<u8>, GenericError> {
            Ok(vec![7; 32])
        }
    }

    #[test]
    fn interrupted_pairing_cleanup_recovers_after_reopen_and_preserves_completed_peers() {
        futures::executor::block_on(async {
            let platform = Arc::new(StubPlatform::default());
            let config = SigningHostConfig::new(
                HostInfo { name: "test wallet".into(), icon: None, version: None, platform: crate::latest::HostPlatform::Unknown },
                PlatformInfo::default(), [0; 32], [1; 32], [2; 32], "paseo".into(),
            ).unwrap();
            let services = RuntimeServices::new(platform.clone(), config.host.host_info.clone(), config.people_chain_genesis_hash, config.bulletin_chain_genesis_hash, config.asset_hub_chain_genesis_hash, test_spawner());
            let holder = WalletAccountHolder::new(services.clone(), config.network_suffix.clone());
            holder.activate_local_session(vec![7; 32]).await.unwrap();
            let activation = holder.current_session().unwrap();
            let directory = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(directory.path().join(hex::encode(activation.public_key))).unwrap();
            let database = Db::open(account_core_db_config(directory.path(), &activation.public_key)).await.unwrap();
            let store = RuntimeStore::open(database, platform.clone(), activation.public_key).await.unwrap();
            services.set_runtime_store(Arc::new(store.clone()));
            let abandoned = super::super::PairedSsoPeer { statement_account_id: [0x31; 32], encryption_public_key: [0x41; 32] };
            let completed = super::super::PairedSsoPeer { statement_account_id: [0x32; 32], encryption_public_key: [0x42; 32] };
            let existing = [0x33; 32];
            for peer in [abandoned, completed] {
                remember_pending_pairing(&holder, &services, &activation, peer).await.unwrap();
            }
            let targets = [abandoned.statement_account_id, completed.statement_account_id, existing].map(|account_id| StatementRenewalTarget::Account { account_id, label: hex::encode(account_id) }).to_vec();
            allowance_renewal::track_for_session(&holder, &activation, targets.clone()).await.unwrap();
            for (slot, account) in [abandoned.statement_account_id, completed.statement_account_id, existing].into_iter().enumerate() {
                store.record_allowance(AllowanceRecord { chain: [1; 32], resource: "statement-store".into(), account, allocated_at: 123, priority: None, last_renewed_period: None }, vec![StatementSlotRecord { chain: [1; 32], collection: "People".into(), period: 1, slot: slot as i64, account, priority: 4, last_allocated_or_renewed_at: 123 }]).await.unwrap();
            }
            store.save_paired_host(PairedHostRecord { peer_statement: completed.statement_account_id, peer_encryption: completed.encryption_public_key, status: "paired".into(), added_at: 123, updated_at: 123, outgoing_update_at: None, last_sync_offer_id: None, metadata: Default::default() }).await.unwrap();
            holder.lock().await.unwrap();
            holder.activate_local_session(vec![7; 32]).await.unwrap();
            assert!(cleanup_pending_pairings(&holder, &services, activation.public_key, Some(&activation)).await.is_err());
            assert!(cleanup_pending_pairings(&holder, &services, activation.public_key, None).await.is_err());
            assert_eq!(allowance_renewal::list(&holder).await.unwrap().into_iter().map(|entry| entry.target).collect::<Vec<_>>(), targets);
            holder.lock().await.unwrap();
            drop(holder);
            drop(services);
            drop(store);

            let runtime = SigningHostRuntime::new(platform.clone(), config, test_spawner());
            let prepared = runtime.prepare_wallet(&WalletSecrets, "owner", None).await.unwrap();
            let database = Db::open(account_core_db_config(directory.path(), &prepared.owner_public_key())).await.unwrap();
            let reopened = RuntimeStore::open(database, platform, prepared.owner_public_key()).await.unwrap();
            runtime.set_runtime_store(Arc::new(reopened.clone()));
            runtime.activate_wallet(prepared).await.unwrap();
            assert_eq!(runtime.statement_renewal_targets().await.unwrap().into_iter().map(|entry| entry.target).collect::<Vec<_>>(), targets[1..]);
            let mut retained: Vec<_> = reopened.statement_slots().await.unwrap().into_iter().map(|slot| slot.account).collect();
            retained.sort();
            assert_eq!(retained, vec![completed.statement_account_id, existing]);
            assert_eq!(reopened.allowances().await.unwrap().len(), 2);
            assert_eq!(reopened.paired_hosts().await.unwrap().len(), 1);
            runtime.disconnect_session().await.unwrap();
        });
    }
}
