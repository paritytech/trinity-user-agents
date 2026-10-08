use super::*;

use crate::runtime::allowances::AllowanceResource;
use crate::runtime::authority::AccountGrant;
use futures::FutureExt;
use parity_scale_codec::Encode;
use truapi::api::StatementStore;
use truapi::versioned::statement_store::{
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
};

use crate::host_logic::product_account::{
    derive_full_person_ring_vrf_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::statement_store::SUBMIT_STATEMENT_METHOD;
use crate::runtime::statement_allowance::slot;
use crate::runtime::statement_store::StatementProofFailure;

const PRODUCT_ID: &str = "myapp.dot";
const PERIOD: u32 = 7;
const SECRET: [u8; 64] = [0x42; 64];

const PEOPLE_METADATA: &[u8] =
    include_bytes!("../../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

/// Slot 0 already holds the product's allowance.
fn chain_with_allocated_slot() -> Arc<StubPlatform> {
    let allowance =
        derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT_ID])
            .expect("allowance derivation succeeds");
    let slot_entry = (allowance.public.to_bytes(), 0u32, 0u64).encode();
    let people_row = slot::testing::slot_row(
        derive_full_person_ring_vrf_entropy(&ENTROPY, TEST_NETWORK_SUFFIX),
        TEST_NETWORK_SUFFIX.as_bytes(),
        slot::current_period(crate::unix_time::current_unix_secs()),
        &[Some(format!(r#""0x{}""#, hex::encode(&slot_entry)))],
    );
    Arc::new(StubPlatform {
        rpc_method_responses: vec![
            (
                "state_getRuntimeVersion",
                r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
            ),
            (
                "chain_getBlockHash",
                format!(r#""0x{}""#, hex::encode([0u8; 32])),
            ),
            (
                "Metadata_metadata_at_version",
                format!(
                    r#""0x{}""#,
                    hex::encode(Some(PEOPLE_METADATA.to_vec()).encode()),
                ),
            ),
            (
                "RuntimeViewFunction_execute_view_function",
                format!(
                    r#""0x{}""#,
                    hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode()),
                ),
            ),
            (
                "RuntimeViewFunction_execute_view_function",
                format!(
                    r#""0x{}""#,
                    hex::encode(Ok::<Vec<u8>, ()>(10u32.encode()).encode()),
                ),
            ),
            (
                "state_getStorage",
                format!(r#""0x{}""#, hex::encode(TEST_NETWORK_SUFFIX.encode())),
            ),
            (
                "state_queryStorageAt",
                people_row,
            ),
            // The LitePeople row, read alongside People's, is empty.
            (
                "state_queryStorageAt",
                r#"[{"block":"0xb10c","changes":[]}]"#.to_string(),
            ),
        ],
        ..Default::default()
    })
}

fn active_signing_host(platform: Arc<StubPlatform>) -> Arc<SigningHostRole> {
    let (_services, signing_host) = signing_runtime_with_platform(platform);
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    signing_host
}

/// Bounded so a regression waiting on an unanswered chain read fails instead
/// of hanging.
fn proof_signer(signing_host: &SigningHostRole) -> [u8; 32] {
    let proof = futures::executor::block_on(async {
        let session = signing_host
            .accounts()
            .current_session()
            .expect("a session is active");
        let cx = CallContext::default();
        futures::select! {
            result = signing_host.accounts()
                .create_authorized_statement_proof(
                    &cx,
                    &session,
                    PRODUCT_ID.to_string(),
                    crate::test_support::statement(),
                )
                .fuse() => result,
            _ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
                panic!("the proof blocked on a chain read")
            }
        }
    });
    let Ok(truapi::latest::StatementProof::Sr25519 { signer, .. }) = proof else {
        panic!("the statement must have an Sr25519 proof");
    };
    signer
}

fn sent_rpc_count(platform: &StubPlatform) -> usize {
    platform
        .sent_rpc
        .lock()
        .expect("rpc list mutex poisoned")
        .len()
}

fn secret_key() -> StatementStoreAllowanceKey {
    StatementStoreAllowanceKey::from_secret_bytes(SECRET.to_vec()).expect("64-byte secret")
}

fn remember(signing_host: &SigningHostRole, product_id: &str, period: u32) {
    let state = signing_host.session_state();
    let session = state.current().unwrap();
    let revision = signing_host.grants.lifecycle().revision();
    futures::executor::block_on(signing_host.grants.retain_allowance(
        &state,
        &session,
        revision,
        product_id,
        &AccountGrant::StatementStore {
            key: secret_key(),
            period: Some(period),
        },
    ))
    .unwrap();
}

fn remembered(signing_host: &SigningHostRole, product_id: &str) -> Option<[u8; 64]> {
    let state = signing_host.session_state();
    let session = state.current().unwrap();
    let revision = signing_host.grants.lifecycle().revision();
    futures::executor::block_on(signing_host.grants.cached_allowance(
        &state,
        &session,
        revision,
        product_id,
        AllowanceResource::StatementStore,
    ))
    .unwrap()
    .map(|grant| match grant {
        AccountGrant::StatementStore { key, .. } => key.secret,
        _ => panic!("expected statement-store grant"),
    })
}

#[test]
fn a_second_proof_in_the_same_session_sends_nothing_to_the_chain() {
    let platform = chain_with_allocated_slot();
    let signing_host = active_signing_host(platform.clone());

    let first = proof_signer(&signing_host);
    let sent_after_first = sent_rpc_count(&platform);
    let second = proof_signer(&signing_host);

    assert_eq!(
        (sent_after_first > 0, second, sent_rpc_count(&platform)),
        (true, first, sent_after_first),
        "the first proof must reach the chain, and a proof after it must not"
    );
}

#[test]
fn a_new_period_looks_the_allowance_up_again() {
    let platform = chain_with_allocated_slot();
    let signing_host = active_signing_host(platform.clone());
    let period = slot::current_period(crate::unix_time::current_unix_secs());
    remember(&signing_host, PRODUCT_ID, period.checked_sub(1).unwrap());

    let restarted = active_signing_host(platform.clone());
    let signer = proof_signer(&restarted);
    assert_eq!(
        (
            signer == secret_key().public_key,
            sent_rpc_count(&platform) > 0
        ),
        (false, true),
        "the next period must look up the allowance instead of serving its stale key",
    );
}

#[test]
fn a_new_session_reuses_the_same_wallet_allowance() {
    let platform = Arc::new(StubPlatform::default());
    let signing_host = active_signing_host(platform.clone());
    remember(&signing_host, PRODUCT_ID, PERIOD);

    futures::executor::block_on(signing_host.disconnect());
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec())).unwrap();
    let after_lock = remembered(&signing_host, PRODUCT_ID);
    let restarted = active_signing_host(platform);
    let state = restarted.session_state();
    let session = state.current().unwrap();
    let revision = restarted.grants.lifecycle().revision();
    let restored = futures::executor::block_on(restarted.grants.cached_allowance(
        &state,
        &session,
        revision,
        PRODUCT_ID,
        AllowanceResource::StatementStore,
    ))
    .unwrap();
    futures::executor::block_on(restarted.activate_local_session(vec![0xAC; 16])).unwrap();

    assert_eq!(
        (
            after_lock,
            restored.map(|grant| match grant {
                AccountGrant::StatementStore { key, period } => (period, key.secret),
                _ => panic!("expected statement-store grant"),
            }),
            remembered(&restarted, PRODUCT_ID)
        ),
        (Some(SECRET), Some((Some(PERIOD), SECRET)), None),
        "same-wallet restart preserves the actual period without exposing its key to another owner",
    );
}

#[test]
fn clearing_a_product_forgets_only_its_key() {
    let platform = Arc::new(StubPlatform::default());
    let signing_host = active_signing_host(platform.clone());
    remember(&signing_host, PRODUCT_ID, PERIOD);
    remember(&signing_host, "other.dot", PERIOD);
    futures::executor::block_on(signing_host.activate_local_session(vec![0xAC; 16])).unwrap();
    remember(&signing_host, PRODUCT_ID, PERIOD);
    remember(&signing_host, "other.dot", PERIOD);
    futures::executor::block_on(signing_host.disconnect());
    futures::executor::block_on(signing_host.clear_product_state(PRODUCT_ID)).unwrap();

    let restarted = active_signing_host(platform);
    let first = (
        remembered(&restarted, PRODUCT_ID),
        remembered(&restarted, "other.dot"),
    );
    futures::executor::block_on(restarted.activate_local_session(vec![0xAC; 16])).unwrap();
    let second = (
        remembered(&restarted, PRODUCT_ID),
        remembered(&restarted, "other.dot"),
    );
    assert_eq!(
        (first, second),
        ((None, Some(SECRET)), (None, Some(SECRET))),
        "locked product reset preserves unrelated grants for both owners"
    );
}

#[test]
fn a_replaced_session_is_not_served_the_new_sessions_key() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    let authority_session = signing_host.accounts().current_session().unwrap();
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");
    remember(
        &signing_host,
        PRODUCT_ID,
        slot::current_period(crate::unix_time::current_unix_secs()),
    );

    assert!(
        matches!(
            futures::executor::block_on(signing_host.accounts().create_authorized_statement_proof(
                &CallContext::default(),
                &authority_session,
                PRODUCT_ID.to_string(),
                crate::test_support::statement(),
            )),
            Err(StatementProofFailure::NoSession),
        ),
        "a request validated under the replaced session was served a proof",
    );
}

#[test]
fn a_key_allocated_under_a_replaced_session_is_not_remembered() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    let state = signing_host.session_state();
    let session = state.current().unwrap();
    let revision = signing_host.grants.lifecycle().revision();
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");

    let remembered_stale = futures::executor::block_on(signing_host.grants.retain_allowance(
        &state,
        &session,
        revision,
        PRODUCT_ID,
        &AccountGrant::StatementStore {
            key: secret_key(),
            period: Some(PERIOD),
        },
    ));
    assert_eq!(
        (
            remembered_stale.map(|_| ()),
            remembered(&signing_host, PRODUCT_ID)
        ),
        (Err(AuthorityError::Disconnected), None),
        "a key allocated for a replaced session was handed back or remembered",
    );
}

#[test]
fn product_reset_does_not_restore_a_pending_allowance() {
    let (release, gate) = futures::channel::oneshot::channel();
    let platform = chain_with_allocated_slot();
    *platform.rpc_method_responses_gate.lock().unwrap() = Some(gate);
    let signing_host = active_signing_host(platform.clone());
    let authority_session = signing_host.accounts().current_session().unwrap();
    let cx = CallContext::default();
    let allocation = signing_host.accounts().create_authorized_statement_proof(
        &cx,
        &authority_session,
        PRODUCT_ID.to_string(),
        crate::test_support::statement(),
    );
    futures::pin_mut!(allocation);
    assert!(allocation.as_mut().now_or_never().is_none());
    crate::test_support::wait_until(
        || sent_rpc_count(&platform) > 0,
        "allowance preparation did not reach the chain",
    );
    futures::executor::block_on(signing_host.clear_product_state(PRODUCT_ID)).unwrap();
    release.send(()).unwrap();
    let result = futures::executor::block_on(async {
        futures::select! {
            result = allocation.fuse() => result.map(|_| ()),
            _ = futures_timer::Delay::new(std::time::Duration::from_secs(5)).fuse() => panic!("stale acquisition did not stop"),
        }
    });
    assert_eq!(
        (
            matches!(result, Err(StatementProofFailure::NoSession)),
            remembered(&signing_host, PRODUCT_ID),
            remembered(&active_signing_host(platform), PRODUCT_ID),
        ),
        (true, None, None),
        "a late grant cannot restore the cleared key in memory or storage",
    );
}

/// Every attempt the submit retry makes: the first plus its retries.
const SUBMIT_ATTEMPTS: usize = 11;

fn rejected(reason: &str) -> String {
    format!(r#"{{"status":"rejected","reason":"{reason}"}}"#)
}

fn submitted_statement(signer: [u8; 32]) -> RemoteStatementStoreSubmitRequest {
    RemoteStatementStoreSubmitRequest::V1(truapi::latest::SignedStatement {
        proof: truapi::latest::StatementProof::Sr25519 {
            signature: [0; 64],
            signer,
        },
        decryption_key: None,
        expiry: None,
        channel: None,
        topics: Vec::new(),
        data: None,
    })
}

fn submit_answered(
    answers: Vec<String>,
    signer: [u8; 32],
) -> (
    Result<(), CallError<RemoteStatementStoreSubmitError>>,
    Arc<SigningHostRole>,
) {
    let platform = Arc::new(StubPlatform {
        rpc_method_responses: answers
            .into_iter()
            .map(|answer| (SUBMIT_STATEMENT_METHOD, answer))
            .collect(),
        ..Default::default()
    });
    let (services, signing_host) = signing_runtime_with_platform(platform);
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("activation succeeds");
    remember(&signing_host, PRODUCT_ID, PERIOD);
    let runtime = product_runtime_for(services, signing_host.clone(), PRODUCT_ID);

    let submitted = futures::executor::block_on(
        runtime.submit(&CallContext::default(), submitted_statement(signer)),
    )
    .map(|_| ());
    (submitted, signing_host)
}

fn submit_rejected(reason: &str, attempts: usize, signer: [u8; 32]) -> Arc<SigningHostRole> {
    let (submitted, signing_host) = submit_answered(vec![rejected(reason); attempts], signer);
    let Err(CallError::Domain(RemoteStatementStoreSubmitError::V1(error))) = submitted else {
        panic!("the store rejection must surface as a domain error: {submitted:?}");
    };
    assert!(
        error.reason.contains(reason),
        "the submit failed before the store answered: {}",
        error.reason
    );
    signing_host
}

#[test]
fn a_lasting_no_allowance_rejection_forgets_the_rejected_key() {
    let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, secret_key().public_key);
    futures::executor::block_on(signing_host.disconnect());
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec())).unwrap();

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID),
        None,
        "the next proof would reuse a key the store no longer accepts"
    );
}

#[test]
fn a_no_allowance_rejection_that_clears_on_retry_keeps_the_key() {
    let (submitted, signing_host) = submit_answered(
        vec![rejected("noAllowance"), r#"{"status":"new"}"#.to_string()],
        secret_key().public_key,
    );

    assert_eq!(
        (submitted.is_ok(), remembered(&signing_host, PRODUCT_ID)),
        (true, Some(SECRET)),
        "credit that is only late to reach the store must not cost the product its key"
    );
}

#[test]
fn a_no_allowance_rejection_for_another_signer_keeps_the_key() {
    let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, [0x11; 32]);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID),
        Some(SECRET),
        "a rejection for a key the host did not issue must not evict the cached one"
    );
}

#[test]
fn another_rejection_keeps_the_key() {
    let signing_host = submit_rejected("badProof", 1, secret_key().public_key);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID),
        Some(SECRET),
        "a rejection that says nothing about the allowance must not evict the key"
    );
}

#[cfg(feature = "test-host")]
#[test]
fn native_bulletin_reuses_its_retained_key_until_refresh() {
    use crate::runtime::BulletinAllowanceKey;

    let platform = Arc::new(StubPlatform {
        chain_connect_error: Some("unexpected Bulletin issuer call"),
        ..Default::default()
    });
    let host = active_signing_host(platform.clone());
    host.set_grant_allowances_unchecked(true);
    let retained = derive_sr25519_hard_path(&ENTROPY, &["previous-bulletin-grant"])
        .unwrap()
        .secret
        .to_bytes();
    let issued = derive_sr25519_hard_path(&ENTROPY, &["allowance", "bulletin", PRODUCT_ID])
        .unwrap()
        .secret
        .to_bytes();
    let state = host.session_state();
    let session = state.current().unwrap();
    let authority_session = host.accounts().current_session().unwrap();
    let revision = host.grants.lifecycle().revision();
    let result = futures::executor::block_on(async {
        host.grants
            .retain_allowance(
                &state,
                &session,
                revision,
                PRODUCT_ID,
                &AccountGrant::Bulletin(
                    BulletinAllowanceKey::from_secret_bytes(retained.to_vec()).unwrap(),
                ),
            )
            .await
            .unwrap();
        let (_, cold) = signing_runtime_with_platform(platform.clone());
        cold.activate_local_session(ENTROPY.to_vec()).await.unwrap();
        let cold_session = cold.accounts().current_session().unwrap();
        let cx = CallContext::default();
        let warm = cold
            .accounts()
            .bulletin_allowance_key(&cx, &cold_session, PRODUCT_ID.to_string())
            .await
            .unwrap();
        let refreshed = host
            .accounts()
            .refresh_bulletin_allowance_key(&cx, &authority_session, PRODUCT_ID.to_string())
            .await
            .unwrap();
        let cached = host
            .grants
            .cached_allowance(
                &state,
                &session,
                revision,
                PRODUCT_ID,
                AllowanceResource::Bulletin,
            )
            .await
            .unwrap();
        (
            *warm.as_secret_bytes(),
            *refreshed.as_secret_bytes(),
            cached.map(|grant| match grant {
                AccountGrant::Bulletin(key) => *key.as_secret_bytes(),
                _ => panic!("expected Bulletin grant"),
            }),
        )
    });

    assert_eq!(result, (retained, issued, Some(issued)));
}

#[test]
fn late_rejection_preserves_a_new_period_for_the_same_statement_key() {
    let (release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        rpc_method_responses: vec![
            (SUBMIT_STATEMENT_METHOD, rejected("noAllowance"));
            SUBMIT_ATTEMPTS
        ],
        ..Default::default()
    });
    *platform.rpc_method_responses_gate.lock().unwrap() = Some(gate);
    let (services, host) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(host.activate_local_session(ENTROPY.to_vec())).unwrap();
    remember(&host, PRODUCT_ID, PERIOD);
    let runtime = product_runtime_for(services, host.clone(), PRODUCT_ID);
    let cx = CallContext::default();
    let submitted = runtime.submit(&cx, submitted_statement(secret_key().public_key));
    futures::pin_mut!(submitted);
    assert!(submitted.as_mut().now_or_never().is_none());
    crate::test_support::wait_until(
        || sent_rpc_count(&platform) > 0,
        "submission did not reach the chain",
    );
    remember(&host, PRODUCT_ID, PERIOD + 1);
    release.send(()).unwrap();
    let result = futures::executor::block_on(submitted);
    assert!(
        matches!(result, Err(CallError::Domain(RemoteStatementStoreSubmitError::V1(error))) if error.reason.contains("noAllowance"))
    );

    let restarted = active_signing_host(platform);
    let state = restarted.session_state();
    let session = state.current().unwrap();
    let revision = restarted.grants.lifecycle().revision();
    let retained = futures::executor::block_on(restarted.grants.cached_allowance(
        &state,
        &session,
        revision,
        PRODUCT_ID,
        AllowanceResource::StatementStore,
    ))
    .unwrap();
    assert_eq!(
        retained.map(|grant| match grant {
            AccountGrant::StatementStore { key, period } => (period, key.secret),
            _ => panic!("expected statement-store grant"),
        }),
        Some((Some(PERIOD + 1), SECRET))
    );
}

#[test]
fn started_native_writes_survive_lock_but_not_an_interrupted_product_reset() {
    for change in ["lock", "reset-and-drop-write", "reset"] {
        let platform = Arc::new(StubPlatform::default());
        let host = active_signing_host(platform.clone());
        remember(&host, "other.dot", PERIOD);
        let state = host.session_state();
        let session = state.current().unwrap();
        let revision = host.grants.lifecycle().revision();
        let (release, gate) = futures::channel::oneshot::channel();
        *platform.core_storage_write_gate.lock().unwrap() = Some(gate);
        let grant = AccountGrant::StatementStore {
            key: secret_key(),
            period: Some(PERIOD),
        };
        let mut write = Box::pin(
            host.grants
                .retain_allowance(&state, &session, revision, PRODUCT_ID, &grant),
        );
        assert!(write.as_mut().now_or_never().is_none());
        if change == "lock" {
            futures::executor::block_on(host.disconnect());
        } else {
            let mut reset = Box::pin(host.clear_product_state(PRODUCT_ID));
            assert!(reset.as_mut().now_or_never().is_none());
            drop(reset);
        }
        if change == "reset-and-drop-write" {
            drop(write);
            let _ = release.send(());
        } else {
            release.send(()).unwrap();
            assert_eq!(
                futures::executor::block_on(write).map(|_| ()),
                Err(AuthorityError::Disconnected)
            );
        }
        futures::executor::block_on(host.activate_local_session(ENTROPY.to_vec())).unwrap();
        assert_eq!(
            (
                remembered(&host, PRODUCT_ID),
                remembered(&host, "other.dot")
            ),
            (
                if change == "lock" { Some(SECRET) } else { None },
                Some(SECRET)
            ),
            "{change}",
        );
    }
}

#[test]
fn corrupt_native_records_preserve_scoped_reset_for_a_later_retry() {
    use crate::platform::{CoreStorage, CoreStorageKey};

    let platform = Arc::new(StubPlatform::default());
    let host = active_signing_host(platform.clone());
    remember(&host, PRODUCT_ID, PERIOD);
    remember(&host, "other.dot", PERIOD);
    let slot = CoreStorageKey::NativeAllowanceKeys;
    let valid = futures::executor::block_on(platform.read_core_storage(slot.clone()))
        .unwrap()
        .unwrap();
    let corrupt = vec![0xff];
    futures::executor::block_on(platform.write_core_storage(slot.clone(), corrupt.clone()))
        .unwrap();
    futures::executor::block_on(host.disconnect());
    let reset = futures::executor::block_on(host.clear_product_state(PRODUCT_ID));
    let after = futures::executor::block_on(platform.read_core_storage(slot.clone())).unwrap();
    assert_eq!(
        (reset, after),
        (
            Err(AuthorityError::Unavailable {
                reason: "persisted native allowances are invalid".to_string()
            }),
            Some(corrupt)
        )
    );

    futures::executor::block_on(platform.write_core_storage(slot, valid)).unwrap();
    futures::executor::block_on(host.activate_local_session(ENTROPY.to_vec())).unwrap();
    assert_eq!(
        (
            remembered(&host, PRODUCT_ID),
            remembered(&host, "other.dot")
        ),
        (None, Some(SECRET))
    );
}
