use super::*;
use crate::platform::HostInfo;
use crate::runtime::{AccountHolder, RuntimeServices};
use crate::test_support::{StubPlatform, test_spawner};
use truapi::latest::HostPlatform;

use crate::runtime::signing_host::wallet_account_holder;
fn wallet(suffix: &str) -> WalletAccountHolder {
    let services = RuntimeServices::new(
        Arc::new(StubPlatform::default()),
        HostInfo {
            name: "allowance test".to_string(),
            icon: None,
            version: None,
            platform: HostPlatform::Unknown,
        },
        [0; 32],
        [0; 32],
        [0; 32],
        test_spawner(),
    );
    let registry =
        crate::runtime::ring_vrf_registry::RingVrfRegistryStore::new(services.platform.clone());
    let wallet = WalletAccountHolder::new(services, suffix.to_string(), registry);
    wallet_account_holder::install(
        &wallet,
        wallet_account_holder::prepare_activation(&wallet, vec![7; 32], None).unwrap(),
    );
    wallet
}

#[test]
fn reserved_identities_follow_the_network_and_collection_order() {
    for suffix in ["dot", "paseo"] {
        let wallet = wallet(suffix);
        let session = wallet.current_session().unwrap();
        let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
        let members = PersonhoodCollection::ALL
            .map(|collection| (collection, signer.member(collection).unwrap()));
        let expected = [
            (PersonhoodCollection::People, 0),
            (PersonhoodCollection::LitePeople, 1),
        ]
        .map(|(collection, index)| {
            let entropy = derive_ring_vrf_entropy(
                &[7; 32],
                &format!("peopl.{suffix}"),
                &truapi::latest::DerivationIndex::Index(index),
            )
            .unwrap();
            (collection, signer.vrf.member(&entropy).unwrap())
        });
        assert_eq!(members, expected);
    }
}

#[test]
fn wallet_replacement_during_revision_read_prevents_allowance_submission() {
    use crate::runtime::statement_allowance::rpc::{RpcClient, testing::ScriptedRpc};
    use crate::runtime::statement_allowance::{
        self as allocation, CollectionScan, PooledRegistrationParams, slot,
    };
    use futures::FutureExt;
    use parity_scale_codec::Encode;

    let wallet = wallet("paseo");
    let session = wallet.current_session().unwrap();
    let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
    let ring = RingParams {
        collection: PersonhoodCollection::LitePeople,
        members: vec![signer.member(PersonhoodCollection::LitePeople).unwrap()],
        exponent: 9,
        ring_index: 0,
        block_hash: "0xfinal".to_string(),
    };
    let state = allocation::extension::ChainState {
        spec_version: 1_000_000,
        transaction_version: 1,
        genesis_hash: [0xab; 32],
        nonce: 0,
        restrict_origins: false,
    };
    let verified = format!(
        r#""0x{}""#,
        hex::encode(([0x22u8; 32], 0u32, 1_000u64).encode())
    );
    let scripted = ScriptedRpc::new(["null", verified.as_str()]);
    scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
    let release = scripted.pause_response(0);
    let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
    let scans = [CollectionScan {
        collection: ring.collection,
        selection: slot::SlotSelection::Free(0),
    }];
    let memberships = [ring];
    let registration = allocation::register_statement_account_pooled(
        &rpc,
        allocation::people_test_metadata(),
        &state,
        &signer,
        &scans,
        &memberships,
        PooledRegistrationParams {
            target: &[0x22; 32],
            period: 7,
            network_suffix: b"paseo",
            reuse_existing: true,
            allow_eviction: false,
            protected: &[],
        },
    );
    futures::pin_mut!(registration);
    assert!(registration.as_mut().now_or_never().is_none());
    wallet_account_holder::install(
        &wallet,
        wallet_account_holder::prepare_activation(&wallet, vec![7; 32], None).unwrap(),
    );
    release.send(()).unwrap();
    let result = futures::executor::block_on(registration)
        .map(|_| ())
        .map_err(|error| AllowanceAllocationError::from(error).into_authority_error());
    let methods: Vec<_> = scripted
        .calls()
        .into_iter()
        .map(|(method, _)| method)
        .collect();
    assert_eq!(
        (result, methods),
        (
            Err(AuthorityError::Disconnected),
            vec!["state_getStorage".to_string()]
        )
    );
}

#[test]
fn wallet_lock_after_a_full_pgas_batch_prevents_the_next_batch() {
    use crate::runtime::statement_allowance::rpc::{RpcClient, testing::ScriptedRpc};
    use crate::runtime::statement_allowance::{self as allocation, slot};
    use futures::FutureExt;

    let wallet = wallet("paseo");
    let session = wallet.current_session().unwrap();
    let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
    let responses: Vec<_> = std::iter::repeat_n(r#""0x""#, 10)
        .chain(std::iter::repeat_n("null", 10))
        .collect();
    let scripted = ScriptedRpc::new(responses);
    let release = scripted.pause_response(0);
    let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
    let scan = slot::scan_pgas_slot_excluding(
        &rpc,
        allocation::asset_hub_test_metadata(),
        PersonhoodCollection::LitePeople,
        &signer,
        b"paseo",
        7,
        &[],
    );
    futures::pin_mut!(scan);
    assert!(scan.as_mut().now_or_never().is_none());
    wallet_account_holder::clear(&wallet);
    release.send(()).unwrap();
    let result = futures::executor::block_on(scan)
        .map_err(|error| AllowanceAllocationError::from(error).into_authority_error());
    let methods: Vec<_> = scripted
        .calls()
        .into_iter()
        .map(|(method, _)| method)
        .collect();
    assert_eq!(
        (result, methods),
        (
            Err(AuthorityError::Disconnected),
            vec!["state_queryStorageAt".to_string()]
        )
    );
}

#[test]
fn wallet_replacement_after_the_first_renewal_submission_stops_the_pass() {
    use crate::runtime::statement_allowance::rpc::{RpcClient, testing::ScriptedRpc};
    use crate::runtime::statement_allowance::{
        self as allocation,
        renewal::{RenewalChainContext, ResolvedRenewalTarget, renew_targets},
    };
    use futures::FutureExt;
    use parity_scale_codec::Encode;

    let wallet = wallet("paseo");
    let session = wallet.current_session().unwrap();
    let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
    let rings = [RingParams {
        collection: PersonhoodCollection::LitePeople,
        members: vec![signer.member(PersonhoodCollection::LitePeople).unwrap()],
        exponent: 9,
        ring_index: 0,
        block_hash: "0xfinal".to_string(),
    }];
    let state = allocation::extension::ChainState {
        spec_version: 1_000_000,
        transaction_version: 1,
        genesis_hash: [0xab; 32],
        nonce: 0,
        restrict_origins: false,
    };
    let targets = [
        ResolvedRenewalTarget {
            label: "first".to_string(),
            account_id: [0x21; 32],
        },
        ResolvedRenewalTarget {
            label: "second".to_string(),
            account_id: [0x22; 32],
        },
    ];
    let first = format!(
        r#""0x{}""#,
        hex::encode((targets[0].account_id, 0u32, 1_000u64).encode())
    );
    let second = format!(
        r#""0x{}""#,
        hex::encode((targets[1].account_id, 1u32, 1_000u64).encode())
    );
    let mut responses = vec!["null"; 31];
    responses.extend([first.as_str(), first.as_str()]);
    responses.extend(std::iter::repeat_n("null", 10));
    responses.push(second.as_str());
    let scripted = ScriptedRpc::new(responses);
    scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
    scripted.script_subscription([r#"{"inBlock":"0xb10d"}"#]);
    let release = scripted.pause_response(5);
    let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
    let context = RenewalChainContext {
        rpc: &rpc,
        metadata: allocation::people_test_metadata(),
        chain_state: &state,
        network_suffix: b"paseo",
        collections: &[PersonhoodCollection::LitePeople],
        signer: &signer,
        memberships: &rings,
    };
    let lock = futures::lock::Mutex::new(());
    let renewal = renew_targets(&context, 7, &targets, &lock);
    futures::pin_mut!(renewal);
    assert!(renewal.as_mut().now_or_never().is_none());
    wallet_account_holder::install(
        &wallet,
        wallet_account_holder::prepare_activation(&wallet, vec![8; 32], None).unwrap(),
    );
    release.send(()).unwrap();
    let result = futures::executor::block_on(renewal)
        .map_err(|error| AllowanceAllocationError::from(error).into_authority_error());
    let methods: Vec<_> = scripted
        .calls()
        .into_iter()
        .map(|(method, _)| method)
        .collect();
    assert_eq!(
        (result, methods),
        (
            Err(AuthorityError::Disconnected),
            vec![
                "state_queryStorageAt",
                "state_queryStorageAt",
                "state_queryStorageAt",
                "state_getStorage",
                "author_submitAndWatchExtrinsic",
                "state_getStorage"
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        )
    );
}
