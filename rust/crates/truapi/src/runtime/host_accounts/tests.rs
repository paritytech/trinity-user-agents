use super::*;
use crate::host_logic::product_account::{
    derive_full_person_ring_vrf_entropy, derive_sr25519_hard_path,
};
use crate::host_logic::statement_store::SUBMIT_STATEMENT_METHOD;
use crate::platform::HostInfo;
use crate::runtime::statement_allowance::slot;
use crate::runtime::{LocalActivation, ProductRuntimeHost, WalletAccountHolder};
use crate::store::{Db, DbConfig, DbLocation, RuntimeStore, core_migrations};
use crate::test_support::{StubPlatform, test_spawner};
use parity_scale_codec::Encode;
use truapi::api::StatementStore;
use truapi::versioned::statement_store::{
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
};

const PRODUCT_ID: &str = "myapp.dot";
const SECRET: [u8; 64] = [0x42; 64];
const ENTROPY: [u8; 16] = [0xAB; 16];
const TEST_NETWORK_SUFFIX: &str = "paseo";

fn active_accounts(platform: Arc<StubPlatform>) -> Arc<HostAccounts<WalletAccountHolder>> {
    futures::executor::block_on(async {
        let services = RuntimeServices::new(
            platform.clone(),
            HostInfo {
                name: "test".to_string(),
                icon: None,
                version: None,
                platform: HostPlatform::Unknown,
            },
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            test_spawner(),
        );
        let holder = WalletAccountHolder::new(services.clone(), TEST_NETWORK_SUFFIX.to_string());
        holder
            .activate_local_session(ENTROPY.to_vec())
            .await
            .unwrap();
        let owner = holder.current_session().unwrap().public_key;
        let database = Db::open(DbConfig {
            location: DbLocation::Memory,
            migrations: core_migrations,
            readers: 1,
        })
        .await
        .unwrap();
        services.set_runtime_store(Arc::new(
            RuntimeStore::open(database, platform, owner).await.unwrap(),
        ));
        HostAccounts::native(holder, services)
    })
}

fn secret_key() -> StatementStoreAllowanceKey {
    StatementStoreAllowanceKey::from_secret_bytes(SECRET.to_vec()).unwrap()
}

fn remember(accounts: &HostAccounts<WalletAccountHolder>, product: &str) {
    let session = accounts.current_session().unwrap();
    futures::executor::block_on(accounts.retain_grant(
        &CallContext::default(),
        &session,
        accounts.generation.load(Ordering::SeqCst),
        product,
        AccountGrant::StatementStore(secret_key()),
    ))
    .unwrap();
}

fn remembered(accounts: &HostAccounts<WalletAccountHolder>, product: &str) -> Option<[u8; 64]> {
    let session = accounts.current_session().unwrap();
    futures::executor::block_on(accounts.material(
        &session,
        product,
        &AllocatableResource::StatementStoreAllowance,
    ))
    .unwrap()
    .map(|material| match material {
        StoredMaterial::StatementStore(secret) => secret.0.as_slice().try_into().unwrap(),
        _ => panic!("wrong resource"),
    })
}

#[test]
fn native_allowance_survives_lock_and_host_recreation_but_requires_matching_owner() {
    let platform = Arc::new(StubPlatform::default());
    let accounts = active_accounts(platform);
    remember(&accounts, PRODUCT_ID);
    let stale = accounts.current_session().unwrap();
    futures::executor::block_on(accounts.disconnect()).unwrap();
    assert_eq!(
        futures::executor::block_on(accounts.material(
            &stale,
            PRODUCT_ID,
            &AllocatableResource::StatementStoreAllowance
        ))
        .err(),
        Some(AuthorityError::Disconnected)
    );
    futures::executor::block_on(accounts.holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    let reopened = HostAccounts::native(accounts.holder.clone(), accounts.services.clone());
    assert_eq!(remembered(&reopened, PRODUCT_ID), Some(SECRET));
    futures::executor::block_on(reopened.holder.activate_local_session(vec![0xCD; 16])).unwrap();
    assert_eq!(remembered(&reopened, PRODUCT_ID), None);
    futures::executor::block_on(reopened.holder.activate_local_session(ENTROPY.to_vec())).unwrap();
    assert_eq!(remembered(&reopened, PRODUCT_ID), Some(SECRET));
}

#[test]
fn clearing_one_product_preserves_others_and_rejects_its_late_allocation() {
    let accounts = active_accounts(Arc::new(StubPlatform::default()));
    remember(&accounts, PRODUCT_ID);
    remember(&accounts, "other.dot");
    let session = accounts.current_session().unwrap();
    let generation = accounts.generation.load(Ordering::SeqCst);
    futures::executor::block_on(accounts.clear_product_state(PRODUCT_ID)).unwrap();
    let late = futures::executor::block_on(accounts.retain_grant(
        &CallContext::default(),
        &session,
        generation,
        PRODUCT_ID,
        AccountGrant::StatementStore(secret_key()),
    ));
    assert_eq!(
        (
            late.err(),
            remembered(&accounts, PRODUCT_ID),
            remembered(&accounts, "other.dot")
        ),
        (Some(AuthorityError::Disconnected), None, Some(SECRET))
    );
}

#[test]
fn corrupted_protected_grants_are_errors_and_do_not_trigger_new_wallet_approval() {
    let platform = Arc::new(StubPlatform::default());
    let accounts = active_accounts(platform.clone());
    let session = accounts.current_session().unwrap();
    futures::executor::block_on(
        accounts
            .services
            .platform
            .write_secret_core_storage(accounts.storage_key(&session).unwrap(), vec![0xff]),
    )
    .unwrap();
    let result = futures::executor::block_on(accounts.statement_store_allowance_key(
        &CallContext::default(),
        &session,
        PRODUCT_ID.to_string(),
    ));
    assert!(matches!(result, Err(AuthorityError::Unavailable { .. })));
    assert!(
        platform
            .resource_allocation_reviews
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn journaled_current_period_reuses_protected_key_without_chain_reads() {
    let platform = chain_with_allocated_slot();
    let accounts = active_accounts(platform.clone());
    let session = accounts.current_session().unwrap();
    let first = futures::executor::block_on(accounts.statement_store_allowance_key(
        &CallContext::default(),
        &session,
        PRODUCT_ID.to_string(),
    ))
    .unwrap();
    let reads = platform.sent_rpc.lock().unwrap().len();
    let reopened = HostAccounts::native(accounts.holder.clone(), accounts.services.clone());
    let second = futures::executor::block_on(reopened.statement_store_allowance_key(
        &CallContext::default(),
        &session,
        PRODUCT_ID.to_string(),
    ))
    .unwrap();
    assert_eq!(
        (
            reads > 0,
            second.public_key,
            platform.sent_rpc.lock().unwrap().len()
        ),
        (true, first.public_key, reads)
    );
}

const PEOPLE_METADATA: &[u8] =
    include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

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
            ("state_queryStorageAt", people_row),
            // The LitePeople row, read alongside People's, is empty.
            (
                "state_queryStorageAt",
                r#"[{"block":"0xb10c","changes":[]}]"#.to_string(),
            ),
        ],
        ..Default::default()
    })
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
    Arc<HostAccounts<WalletAccountHolder>>,
) {
    let platform = Arc::new(StubPlatform {
        rpc_method_responses: answers
            .into_iter()
            .map(|answer| (SUBMIT_STATEMENT_METHOD, answer))
            .collect(),
        ..Default::default()
    });
    let signing_host = active_accounts(platform);
    remember(&signing_host, PRODUCT_ID);
    let runtime = ProductRuntimeHost::from_services(
        signing_host.services.clone(),
        crate::host_core::ConnectionAdapters::from_services(&signing_host.services),
        signing_host.clone(),
        ProductContext::new(PRODUCT_ID.to_string()).unwrap(),
    );

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

fn submit_rejected(
    reason: &str,
    attempts: usize,
    signer: [u8; 32],
) -> Arc<HostAccounts<WalletAccountHolder>> {
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

struct PausedRingValidation {
    entered: Mutex<Option<futures::channel::oneshot::Sender<()>>>,
    resume: Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
}

#[async_trait::async_trait]
impl RingResolver for PausedRingValidation {
    async fn members_pallet_index(&self, _chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
        unreachable!("alias validation does not discover the pallet")
    }

    async fn validate(&self, _location: &RingLocation) -> Result<[u8; 32], RingVrfError> {
        self.entered
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .unwrap();
        let resume = self.resume.lock().unwrap().take().unwrap();
        resume.await.unwrap();
        Ok([0; 32])
    }

    async fn resolve(
        &self,
        _location: &RingLocation,
        _candidates: &[MemberCandidate],
    ) -> Result<super::super::signing_host::ring_vrf::ResolvedRing, RingVrfError> {
        unreachable!("alias validation does not resolve membership")
    }
}

#[test]
fn reset_revokes_delegated_alias_work_waiting_for_chain_validation() {
    use crate::host_logic::product_account::{
        derive_product_subtree_keypair, derive_ring_vrf_domain_entropy,
        derive_root_keypair_from_entropy,
    };
    use futures::FutureExt;

    for reset_account in [false, true] {
        let mut accounts = active_accounts(Arc::new(StubPlatform::default()));
        let (entered, reached_validation) = futures::channel::oneshot::channel();
        let (resume, validation_resumed) = futures::channel::oneshot::channel();
        Arc::get_mut(&mut accounts).unwrap().ring_resolver = Arc::new(PausedRingValidation {
            entered: Mutex::new(Some(entered)),
            resume: Mutex::new(Some(validation_resumed)),
        });
        futures::executor::block_on(async {
            let session = accounts.current_session().unwrap();
            let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
            let subtree = derive_product_subtree_keypair(&root, PRODUCT_ID).unwrap();
            let domain = derive_ring_vrf_domain_entropy(&ENTROPY, PRODUCT_ID).unwrap();
            accounts
                .retain_grant(
                    &CallContext::default(),
                    &session,
                    accounts.generation.load(Ordering::SeqCst),
                    PRODUCT_ID,
                    AccountGrant::DelegatedSigning(AutoSigningKey::from_parts(
                        subtree.secret.to_bytes(),
                        domain,
                    )),
                )
                .await
                .unwrap();
            let crate::versioned::account::HostAccountGetAliasRequest::V1(request) =
                crate::test_support::account_alias_request(PRODUCT_ID);
            let entropy =
                derive_ring_vrf_entropy_from_domain(&domain, &request.key_handle.derivation_index);
            accounts
                .ring_vrf_registry
                .register(
                    session.public_key,
                    request.key_handle.clone(),
                    request.ring_location.clone(),
                    vrf::load().await.unwrap().member(&entropy).unwrap(),
                )
                .await
                .unwrap();
            let cx = CallContext::default();
            let operation = accounts.account_alias(
                &cx,
                &session,
                ProductRequest {
                    calling_product_id: PRODUCT_ID.to_string(),
                    payload: request,
                },
            );
            futures::pin_mut!(operation);
            assert!(operation.as_mut().now_or_never().is_none());
            assert_eq!(reached_validation.now_or_never(), Some(Ok(())));
            if reset_account {
                accounts.reset_grants().await.unwrap();
            } else {
                accounts.clear_product_state(PRODUCT_ID).await.unwrap();
            }
            resume.send(()).unwrap();
            assert_eq!(
                operation.await,
                Err(RingVrfError::from(AuthorityError::Disconnected))
            );
        });
    }
}
