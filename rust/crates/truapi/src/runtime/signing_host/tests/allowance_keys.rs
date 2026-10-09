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
        resource_allocation_confirmed: true,
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
    let revision = signing_host.accounts().grants_for_tests().lifecycle().revision();
    futures::executor::block_on(signing_host.accounts().grants_for_tests().retain_allowance(
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
    let revision = signing_host.accounts().grants_for_tests().lifecycle().revision();
    futures::executor::block_on(signing_host.accounts().grants_for_tests().cached_allowance(
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
fn a_new_period_keeps_the_sponsorship_key_until_submission() {
    let platform = chain_with_allocated_slot();
    let signing_host = active_signing_host(platform.clone());
    let period = slot::current_period(crate::unix_time::current_unix_secs());
    remember(&signing_host, PRODUCT_ID, period.checked_sub(1).unwrap());

    let signer = proof_signer(&signing_host);
    assert_eq!(
        (
            signer == secret_key().public_key,
            sent_rpc_count(&platform) > 0
        ),
        (true, false),
        "proof creation reuses the key; submission renews its sponsorship",
    );
}

#[test]
fn a_new_session_looks_the_allowance_up_again() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    remember(&signing_host, PRODUCT_ID, PERIOD);

    futures::executor::block_on(signing_host.disconnect());
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID),
        None,
        "the second session served the first session's key"
    );
}

#[test]
fn clearing_a_product_forgets_only_its_key() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    remember(&signing_host, PRODUCT_ID, PERIOD);
    remember(&signing_host, "other.dot", PERIOD);

    futures::executor::block_on(signing_host.clear_product_state(PRODUCT_ID))
        .expect("the product id is valid");

    assert_eq!(
        (
            remembered(&signing_host, PRODUCT_ID),
            remembered(&signing_host, "other.dot"),
        ),
        (None, Some(SECRET)),
        "clearing a product's state must forget its key and keep the others"
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
    let revision = signing_host.accounts().grants_for_tests().lifecycle().revision();
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");

    let remembered_stale = futures::executor::block_on(signing_host.accounts().grants_for_tests().retain_allowance(
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
        ),
        (true, None),
        "a late grant cannot restore the cleared key in memory",
    );
}

/// Every attempt the submit retry makes: the first plus its retries.
const SUBMIT_ATTEMPTS: usize = 11;

fn rejected(reason: &str) -> String {
    format!(r#"{{"status":"rejected","reason":"{reason}"}}"#)
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
    signing_host.set_grant_allowances_unchecked(true);
    let runtime = product_runtime_for(services, signing_host.clone(), PRODUCT_ID);

    let submitted = futures::executor::block_on(runtime.submit(
        &CallContext::default(),
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
        }),
    ))
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
fn a_rejected_submission_keeps_the_key_for_sponsorship_renewal() {
    let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, secret_key().public_key);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID),
        Some(SECRET),
        "submission failure must not discard an issued sponsorship key"
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
fn native_bulletin_submission_uses_the_retained_key() {
    use crate::runtime::BulletinAllowanceKey;

    let host = active_signing_host(Arc::new(StubPlatform::default()));
    host.set_grant_allowances_unchecked(true);
    let retained = derive_sr25519_hard_path(&ENTROPY, &["previous-bulletin-grant"])
        .unwrap()
        .secret
        .to_bytes();
    let state = host.session_state();
    let session = state.current().unwrap();
    let authority_session = host.accounts().current_session().unwrap();
    let revision = host.accounts().grants_for_tests().lifecycle().revision();
    let result = futures::executor::block_on(async {
        host.accounts().grants_for_tests()
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
        let cx = CallContext::default();
        host.accounts()
            .submit_preimage(
                &cx,
                std::time::Instant::now() + std::time::Duration::from_secs(1),
                &authority_session,
                PRODUCT_ID.to_string(),
                b"retained key",
            )
            .await
            .unwrap();
        let cached = host
            .accounts().grants_for_tests()
            .cached_allowance(
                &state,
                &session,
                revision,
                PRODUCT_ID,
                AllowanceResource::Bulletin,
            )
            .await
            .unwrap();
        cached.and_then(|grant| match grant {
            AccountGrant::Bulletin(key) => Some(*key.as_secret_bytes()),
            _ => None,
        })
    });

    assert_eq!(result, Some(retained));
}

#[test]
fn only_a_retained_sponsorship_renews_without_another_approval() {
    let mut platform = chain_with_allocated_slot();
    Arc::get_mut(&mut platform)
        .unwrap()
        .resource_allocation_confirmed = false;
    let host = active_signing_host(platform.clone());
    let key =
        derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", PRODUCT_ID]).unwrap();
    let state = host.session_state();
    let session = state.current().unwrap();
    let revision = host.accounts().grants_for_tests().lifecycle().revision();
    let product = crate::platform::ProductContext::new(PRODUCT_ID.to_string()).unwrap();
    futures::executor::block_on(async {
        host.accounts().grants_for_tests()
            .retain_allowance(
                &state,
                &session,
                revision,
                PRODUCT_ID,
                &AccountGrant::StatementStore {
                    key: StatementStoreAllowanceKey::from_secret_bytes(
                        key.secret.to_bytes().to_vec(),
                    )
                    .unwrap(),
                    period: Some(PERIOD),
                },
            )
            .await
            .unwrap();
        host.accounts()
            .renew_statement_sponsorship(&CallContext::default(), &product, [0x11; 32])
            .await
            .unwrap();
        assert_eq!(sent_rpc_count(&platform), 0);
        host.accounts()
            .renew_statement_sponsorship(&CallContext::default(), &product, key.public.to_bytes())
            .await
            .unwrap();
    });
    assert_eq!(
        (
            sent_rpc_count(&platform) > 0,
            platform.resource_allocation_reviews.lock().unwrap().len(),
            remembered(&host, PRODUCT_ID)
        ),
        (true, 0, Some(key.secret.to_bytes()))
    );
}

#[test]
fn a_declined_bulletin_extension_keeps_the_retained_key() {
    let platform = Arc::new(StubPlatform {
        rpc_method_responses: vec![("state_getStorage", "null".to_string())],
        ..Default::default()
    });
    let host = active_signing_host(platform.clone());
    let state = host.session_state();
    let session = state.current().unwrap();
    let authority_session = host.accounts().current_session().unwrap();
    let revision = host.accounts().grants_for_tests().lifecycle().revision();
    let key = derive_sr25519_hard_path(&ENTROPY, &["retained-bulletin"]).unwrap();
    let outcome = futures::executor::block_on(async {
        host.accounts().grants_for_tests()
            .retain_allowance(
                &state,
                &session,
                revision,
                PRODUCT_ID,
                &AccountGrant::Bulletin(
                    crate::runtime::BulletinAllowanceKey::from_secret_bytes(
                        key.secret.to_bytes().to_vec(),
                    )
                    .unwrap(),
                ),
            )
            .await
            .unwrap();
        let submission = host
            .accounts()
            .submit_preimage(
                &CallContext::default(),
                std::time::Instant::now() + std::time::Duration::from_secs(1),
                &authority_session,
                PRODUCT_ID.to_string(),
                b"quota required",
            )
            .await;
        let retained = host
            .accounts().grants_for_tests()
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
            matches!(
                submission,
                Err(
                    crate::runtime::bulletin_rpc::BulletinSubmitError::Authority(
                        AuthorityError::Rejected
                    )
                )
            ),
            retained.and_then(|grant| match grant {
                AccountGrant::Bulletin(key) => Some(*key.as_secret_bytes()),
                _ => None,
            }),
        )
    });
    assert_eq!(
        (
            outcome,
            platform.resource_allocation_reviews.lock().unwrap().len()
        ),
        ((true, Some(key.secret.to_bytes())), 1)
    );
}
