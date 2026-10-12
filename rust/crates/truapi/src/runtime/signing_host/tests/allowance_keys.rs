use super::*;

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
fn allowance_key(signing_host: &SigningHostRole) -> StatementStoreAllowanceKey {
    futures::executor::block_on(async {
        let session = signing_host
            .current_session()
            .expect("a session is active");
        let cx = CallContext::default();
        futures::select! {
            result = signing_host
                .statement_store_allowance_key(&cx, &session, PRODUCT_ID.to_string())
                .fuse() => result,
            _ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
                panic!("the proof blocked on a chain read")
            }
        }
    })
    .expect("the proof is served")
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
    signing_host
        .local_grants
        .lock()
        .expect("local AutoSigning grant mutex poisoned")
        .statement_allowance_keys
        .insert(product_id.to_string(), (period, secret_key()));
}

fn current_generation(signing_host: &SigningHostRole) -> u64 {
    signing_host
        .local_grants
        .lock()
        .expect("local AutoSigning grant mutex poisoned")
        .activation_generation
}

fn remembered(signing_host: &SigningHostRole, product_id: &str, period: u32) -> Option<[u8; 64]> {
    let state = signing_host
        .local_grants
        .lock()
        .expect("local AutoSigning grant mutex poisoned");
    state
        .statement_allowance_key(state.activation_generation, product_id, period)
        .expect("the generation is current")
        .map(|key| key.secret)
}

#[test]
fn a_second_proof_in_the_same_session_sends_nothing_to_the_chain() {
    let platform = chain_with_allocated_slot();
    let signing_host = active_signing_host(platform.clone());

    let first = allowance_key(&signing_host);
    let sent_after_first = sent_rpc_count(&platform);
    let second = allowance_key(&signing_host);

    assert_eq!(
        (sent_after_first > 0, second.public_key, sent_rpc_count(&platform)),
        (true, first.public_key, sent_after_first),
        "the first proof must reach the chain, and a proof after it must not"
    );
}

#[test]
fn a_new_period_looks_the_allowance_up_again() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    remember(&signing_host, PRODUCT_ID, PERIOD);

    assert_eq!(
        (
            remembered(&signing_host, PRODUCT_ID, PERIOD),
            remembered(&signing_host, PRODUCT_ID, PERIOD + 1),
        ),
        (Some(SECRET), None),
        "the next period served the previous period's key"
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
        remembered(&signing_host, PRODUCT_ID, PERIOD),
        None,
        "the second session served the first session's key"
    );
}

#[test]
fn clearing_a_product_forgets_only_its_key() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    remember(&signing_host, PRODUCT_ID, PERIOD);
    remember(&signing_host, "other.dot", PERIOD);

    signing_host
        .clear_product_state(PRODUCT_ID)
        .expect("the product id is valid");

    assert_eq!(
        (
            remembered(&signing_host, PRODUCT_ID, PERIOD),
            remembered(&signing_host, "other.dot", PERIOD),
        ),
        (None, Some(SECRET)),
        "clearing a product's state must forget its key and keep the others"
    );
}

#[test]
fn a_replaced_session_is_not_served_the_new_sessions_key() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    let stale_generation = current_generation(&signing_host);
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");
    remember(&signing_host, PRODUCT_ID, PERIOD);

    assert!(
        matches!(
            signing_host
                .local_grants
                .lock()
                .expect("local AutoSigning grant mutex poisoned")
                .statement_allowance_key(stale_generation, PRODUCT_ID, PERIOD),
            Err(AuthorityError::Disconnected)
        ),
        "a request validated under the replaced session was served a key"
    );
}

#[test]
fn a_key_allocated_under_a_replaced_session_is_not_remembered() {
    let signing_host = active_signing_host(Arc::new(StubPlatform::default()));
    let stale_generation = current_generation(&signing_host);
    futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
        .expect("re-activation succeeds");

    let remembered_stale = signing_host
        .local_grants
        .lock()
        .expect("local AutoSigning grant mutex poisoned")
        .remember_statement_allowance_key(
            stale_generation,
            PRODUCT_ID.to_string(),
            PERIOD,
            secret_key(),
        );

    assert_eq!(
        (remembered_stale, remembered(&signing_host, PRODUCT_ID, PERIOD)),
        (Err(AuthorityError::Disconnected), None),
        "a key allocated for a replaced session was handed back or remembered"
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
fn a_lasting_no_allowance_rejection_forgets_the_rejected_key() {
    let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, secret_key().public_key);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID, PERIOD),
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
        (submitted.is_ok(), remembered(&signing_host, PRODUCT_ID, PERIOD)),
        (true, Some(SECRET)),
        "credit that is only late to reach the store must not cost the product its key"
    );
}

#[test]
fn a_no_allowance_rejection_for_another_signer_keeps_the_key() {
    let signing_host = submit_rejected("noAllowance", SUBMIT_ATTEMPTS, [0x11; 32]);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID, PERIOD),
        Some(SECRET),
        "a rejection for a key the host did not issue must not evict the cached one"
    );
}

#[test]
fn another_rejection_keeps_the_key() {
    let signing_host = submit_rejected("badProof", 1, secret_key().public_key);

    assert_eq!(
        remembered(&signing_host, PRODUCT_ID, PERIOD),
        Some(SECRET),
        "a rejection that says nothing about the allowance must not evict the key"
    );
}
