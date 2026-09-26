//! Shared runtime fixtures and cross-capability integration tests.

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use parity_scale_codec::Encode;
use truapi::api::{
    Account, Chain, Entropy, LocalStorage, Notifications, Permissions, Preimage,
    ResourceAllocation, Signing, StatementStore, System, Theme, Worker,
};
use truapi::v02;
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError,
    HostAccountCreateProofResponse, HostAccountGetAliasError, HostAccountGetAliasResponse,
    HostAccountGetRequest, HostAccountGetResponse, HostAccountRingVrfSignError,
    HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse, HostAccountSignVrfRequest,
    HostAccountSignVrfResponse, HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse,
    HostGetUserIdError, HostGetUserIdRequest, HostGetUserIdResponse,
};
use truapi::versioned::chain::{
    RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse,
    RemoteChainTransactionBroadcastError, RemoteChainTransactionBroadcastRequest,
};
use truapi::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use truapi::versioned::local_storage::{
    HostLocalStorageChangeItem, HostLocalStorageClearRequest, HostLocalStorageReadError,
    HostLocalStorageReadRequest, HostLocalStorageReadResponse, HostLocalStorageSubscribeError,
    HostLocalStorageSubscribeRequest, HostLocalStorageWriteRequest,
};
use truapi::versioned::notifications::{
    HostPushNotificationCancelRequest, HostPushNotificationCancelResponse,
    HostPushNotificationError, HostPushNotificationRequest, HostPushNotificationResponse,
};
use truapi::versioned::permissions::{HostDevicePermissionRequest, HostDevicePermissionResponse};
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    RemotePreimageSubmitRequest,
};
use truapi::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use truapi::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignPayloadWithLegacyAccountError,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawError, HostSignRawRequest,
    HostSignRawResponse, HostSignRawWithLegacyAccountError, HostSignRawWithLegacyAccountRequest,
    HostSignRawWithLegacyAccountResponse,
};
use truapi::versioned::statement_store::{
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
};
use truapi::versioned::system::{
    HostFeatureSupportedRequest, HostFeatureSupportedResponse, HostGetProductContextRequest,
    HostGetProductContextResponse, HostNavigateToError, HostNavigateToRequest,
    HostNavigateToResponse,
};
use truapi::versioned::theme::HostThemeSubscribeItem;
use truapi::versioned::worker::{
    HostWorkerBeginOperationRequest, HostWorkerBeginOperationResponse,
    HostWorkerEndOperationRequest,
};
use truapi_platform::{
    AuthState, CoreStorage as PlatformCoreStorage, CoreStorageKey, PermissionAuthorizationRequest,
};

use super::product_manifest::{CachedManifest, MANIFEST_TTL_SECS};
use super::*;
use crate::host_logic::product_account::index_bytes;
use crate::host_logic::product_manifest::test_manifest_json;
use crate::host_logic::sso::messages::{RemoteMessage, RemoteMessageData, Response, v1};
use crate::host_logic::statement_store::current_unix_secs;
use crate::test_support::*;

fn test_product_subtree(product_id: &str) -> [u8; 32] {
    let root = crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16])
        .expect("test entropy derives a root");
    crate::host_logic::product_account::derive_product_subtree_keypair(&root, product_id)
        .expect("test product id derives a subtree")
        .public
        .to_bytes()
}

fn test_product_account_public(product_id: &str, index: u32) -> [u8; 32] {
    derive_product_public_key(test_product_subtree(product_id), index_bytes(index))
        .expect("test subtree derives an account")
}

fn install_pairing_session(host: &ProductRuntimeHost, session: SessionInfo) {
    let product_id =
        normalize_product_identifier(&host.product_id()).expect("test product identifier is valid");
    if session.sso.is_some() {
        host.test_cache_product_subtree(&session, &product_id, test_product_subtree(&product_id));
    }
    host.test_session_state().set_session(session);
}

fn cache_test_product_subtree(host: &ProductRuntimeHost, session: &SessionInfo, product_id: &str) {
    host.test_cache_product_subtree(session, product_id, test_product_subtree(product_id));
}

#[test]
fn preimage_reports_bulletin_allocation_rejection_with_context() {
    assert_eq!(
        bulletin_allowance_error_reason(AuthorityError::Rejected),
        "Bulletin allowance allocation was rejected by the signing host"
    );
}

fn recorded_rpc_methods(sent_rpc: &Mutex<Vec<String>>) -> Vec<String> {
    sent_rpc
        .lock()
        .expect("rpc list mutex poisoned")
        .iter()
        .map(|request| {
            serde_json::from_str::<serde_json::Value>(request).unwrap()["method"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

fn recorded_rpc_method_count(sent_rpc: &Mutex<Vec<String>>, method: &str) -> usize {
    recorded_rpc_methods(sent_rpc)
        .iter()
        .filter(|candidate| candidate.as_str() == method)
        .count()
}

#[test]
fn feature_supported_round_trips_through_runtime() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
        genesis_hash: vec![0u8; 32],
    });
    let response = futures::executor::block_on(host.feature_supported(&cx, request)).unwrap();
    let HostFeatureSupportedResponse::V1(inner) = response;
    assert!(inner.supported);
}

#[test]
fn get_product_context_returns_the_runtime_canonical_product_id() {
    for (configured, expected) in [
        (" TrUAPI-Playground.DOT ", "truapi-playground.dot"),
        ("truapi-playground.paseo", "truapi-playground.paseo"),
        ("truapi-playground.testnet", "truapi-playground.testnet"),
        ("localhost", "localhost"),
        ("LOCALHOST:3000", "localhost:3000"),
    ] {
        let host =
            ProductRuntimeHost::new(stub_platform(), runtime_config(configured), test_spawner());
        let response = futures::executor::block_on(
            host.get_product_context(&CallContext::default(), HostGetProductContextRequest::V1),
        )
        .unwrap();
        let HostGetProductContextResponse::V1(context) = response;

        assert_eq!(
            context,
            v01::HostGetProductContextResponse {
                product_id: expected.to_string(),
            },
            "configured product id {configured:?}",
        );
    }
}

#[test]
fn get_chain_info_round_trips_through_runtime() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = RemoteChainInfoRequest::V1(v01::RemoteChainInfoRequest {
        chain: v01::ChainIdentifier::AssetHub,
    });
    let response = futures::executor::block_on(host.get_chain_info(&cx, request)).unwrap();
    let RemoteChainInfoResponse::V1(inner) = response;
    assert_eq!(inner.network, "paseo");
    assert_eq!(inner.chain, v01::ChainIdentifier::AssetHub);
    assert_eq!(inner.genesis_hash, [0xaa; 32]);
}

fn read_storage(
    host: &ProductRuntimeHost,
    product: Option<&str>,
    key: &str,
) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
    futures::executor::block_on(LocalStorage::read(
        host,
        &CallContext::default(),
        HostLocalStorageReadRequest::V2(v02::HostLocalStorageReadRequest {
            product: product.map(str::to_string),
            key: key.to_string(),
        }),
    ))
}

#[test]
fn a_storage_key_is_namespaced_under_its_owner() {
    // The owner is an argument, not `self`: keying off the caller would hand a
    // granted foreign read the caller's own values under the target's name.
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let key = host.product_storage_key("wallet.dot", "k".to_string());
    let decoded = ProductStorageKey::decode(&key).expect("the key round-trips");
    assert_eq!(decoded.product_id(), "wallet.dot");
    assert_ne!(decoded.product_id(), host.product_id());
}

#[test]
fn a_read_naming_the_caller_however_it_is_spelled_reaches_its_own_storage() {
    // `product: None` is what every v0.1 read meant, naming your own id is the
    // same call, and casing is normalized before the comparison. None of the
    // three is a cross-product access, so none consults a grant.
    let platform = stub_platform();
    let storage = platform.clone();
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    let own = host.product_id();
    storage.local_storage.lock().expect("mutex").insert(
        ProductStorageKey::new(&own, "k")
            .expect("the owner normalizes")
            .encode(),
        b"my own value".to_vec(),
    );

    for spelling in [None, Some(own.clone()), Some(own.to_uppercase())] {
        let answered = read_storage(&host, spelling.as_deref(), "k")
            .unwrap_or_else(|err| panic!("{spelling:?} must reach own storage: {err:?}"));
        let HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse { value }) =
            answered
        else {
            panic!("a v2 request answers with a v2 response");
        };
        assert_eq!(
            value.as_deref(),
            Some(&b"my own value"[..]),
            "spelling {spelling:?}"
        );
    }
}

#[test]
fn an_unresolvable_product_is_refused_identically_to_an_ungranted_one() {
    // One answer for every reason, so the call cannot be used to probe which
    // products exist. Two of these are not product ids at all and the third is
    // a perfectly good one; on a host that reaches no chain none of them
    // resolves, and the caller cannot tell that apart from a refused grant.
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    for target in ["not a product", "", "wallet.dot"] {
        assert_eq!(
            read_storage(&host, Some(target), "k").unwrap_err(),
            access_not_granted(),
            "target {target:?}"
        );
    }
}

/// The one refusal every unmet grant answers with.
fn access_not_granted() -> CallError<HostLocalStorageReadError> {
    CallError::Domain(HostLocalStorageReadError::V2(
        v02::HostLocalStorageReadError::AccessNotGranted,
    ))
}

/// Reads `owner`'s stored value at `k`, requiring the grant to admit it.
fn granted_value(host: &ProductRuntimeHost, owner: &str) -> Option<Vec<u8>> {
    let HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse { value }) =
        read_storage(host, Some(owner), "k").expect("the grant admits the read")
    else {
        panic!("a v2 request answers with a v2 response");
    };
    value
}

/// Seeds `owner`'s storage with a value only a granted read can reach.
fn seed_owner_value(platform: &StubPlatform, owner: &str) {
    platform.local_storage.lock().expect("mutex").insert(
        ProductStorageKey::new(owner, "k")
            .expect("the owner normalizes")
            .encode(),
        b"wallet's own value".to_vec(),
    );
}

/// Seeds `owner`'s cached manifest, so a grant resolves without a chain.
fn cache_manifest(platform: &StubPlatform, owner: &str, trusted: &str, age_secs: u64) {
    cache_manifest_entry(platform, owner, Some(test_manifest_json(trusted)), age_secs);
}

/// Seed `owner`'s cached lookup with an explicit `fetched_at`, for tests about
/// the freshness bound itself rather than about grants.
fn cache_manifest_at(platform: &StubPlatform, owner: &str, trusted: &str, fetched_at_secs: u64) {
    let json = format!(
        r#"{{"$v":1,"displayName":"D","description":"d",
                "icon":{{"cid":"c","format":"png"}},"trustedProducts":{trusted}}}"#
    );
    let entry = CachedManifest {
        fetched_at_secs,
        json: Some(json),
    };
    futures::executor::block_on(platform.write_core_storage(
        crate::runtime::product_manifest::manifest_cache_key(owner),
        entry.encode(),
    ))
    .expect("stub core storage accepts the entry");
}

/// Seeds `owner`'s cached lookup, `None` standing for "publishes no manifest".
fn cache_manifest_entry(platform: &StubPlatform, owner: &str, json: Option<String>, age_secs: u64) {
    let entry = CachedManifest {
        fetched_at_secs: current_unix_secs().saturating_sub(age_secs),
        json,
    };
    futures::executor::block_on(platform.write_core_storage(
        crate::runtime::product_manifest::manifest_cache_key(owner),
        entry.encode(),
    ))
    .expect("stub core storage accepts the entry");
}

#[test]
fn a_cached_grant_reads_the_granting_products_storage() {
    // The caller is `unknown.dot`, so the manifest names the bare label.
    //
    // Seeding only the target's namespace is what makes this a regression pin:
    // keying the read off the caller instead would miss the value entirely.
    let platform = stub_platform();
    cache_manifest(&platform, "wallet.dot", r#"{"unknown":["storage"]}"#, 0);
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        granted_value(&host, "wallet.dot").as_deref(),
        Some(&b"wallet's own value"[..])
    );
}

#[test]
fn all_satisfies_a_storage_read() {
    let platform = stub_platform();
    cache_manifest(&platform, "wallet.dot", r#"{"unknown":["all"]}"#, 0);
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        granted_value(&host, "wallet.dot").as_deref(),
        Some(&b"wallet's own value"[..])
    );
}

#[test]
fn a_grant_to_another_product_does_not_admit_this_caller() {
    let platform = stub_platform();
    cache_manifest(&platform, "wallet.dot", r#"{"stash":["storage"]}"#, 0);
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted()
    );
}

#[test]
fn a_cached_miss_refuses_without_returning_to_the_chain() {
    // "This product publishes no manifest" is an answer worth keeping. Without
    // it every refusal re-reads the contracts, and the round trip separates a
    // target that has a manifest from one that does not.
    let platform = stub_platform();
    cache_manifest_entry(&platform, "wallet.dot", None, 0);
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted()
    );
}

#[test]
fn a_grant_of_some_other_scope_does_not_open_storage() {
    // Only `storage` and `all` open a read. A value naming anything else, now
    // or once a later scope is defined, must leave storage refusing.
    let platform = stub_platform();
    cache_manifest(
        &platform,
        "wallet.dot",
        r#"{"unknown":["storage-write"]}"#,
        0,
    );
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted()
    );
}

#[test]
fn a_context_grant_does_not_open_storage() {
    // Scopes are independent, and `context` is the case worth pinning rather
    // than an unrecognised value: it is a scope this core does honour, just
    // not for storage.
    let platform = stub_platform();
    cache_manifest(&platform, "wallet.dot", r#"{"unknown":["context"]}"#, 0);
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted()
    );
}

/// A cache entry stamped in the future is stale, not immortal.
///
/// The TTL is the revocation bound: a grant a publisher withdraws stays in force
/// until the document is read again. An entry written while the device clock ran
/// ahead used to satisfy the bound forever, because `saturating_sub` floors at
/// zero, so that one entry could never be revoked.
#[test]
fn a_cache_entry_stamped_in_the_future_is_not_honoured() {
    let platform = stub_platform();
    // A year ahead: `saturating_sub` gives 0, which is below any TTL.
    cache_manifest_at(
        &platform,
        "wallet.dot",
        r#"{"unknown":["storage"]}"#,
        crate::host_logic::statement_store::current_unix_secs() + 365 * 24 * 60 * 60,
    );
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted(),
        "a future-stamped entry must be re-read, not trusted forever"
    );
}

#[test]
fn a_cached_grant_stops_being_honoured_once_it_expires() {
    // The lifetime is the revocation bound. Past it the entry is ignored,
    // and with no Asset Hub to re-read from the grant is gone.
    let platform = stub_platform();
    cache_manifest(
        &platform,
        "wallet.dot",
        r#"{"unknown":["storage"]}"#,
        MANIFEST_TTL_SECS + 1,
    );
    seed_owner_value(&platform, "wallet.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    assert_eq!(
        read_storage(&host, Some("wallet.dot"), "k").unwrap_err(),
        access_not_granted()
    );
}

/// With no session, every proof refusal is the same refusal.
///
/// This is the ordering hazard closed. `create_account_proof` consults the
/// session before the grant, so a granting target, a non-granting target and
/// the caller's own key all answer `Rejected`, and the pair of refusals stops
/// being a probe for who granted whom. The grant path itself is covered
/// end-to-end, with a live session, in
/// `runtime::signing_host::tests::a_context_grant_lets_a_foreign_product_prove_with_the_owners_key`.
#[test]
fn with_no_session_a_proof_refusal_never_discloses_whether_a_grant_exists() {
    let platform = stub_platform();
    cache_manifest(&platform, "granting.dot", r#"{"unknown":["context"]}"#, 0);
    cache_manifest(
        &platform,
        "silent.dot",
        r#"{"someone-else":["context"]}"#,
        0,
    );
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    let sessionless = Some(CallError::Domain(HostAccountCreateProofError::V1(
        v01::HostAccountCreateProofError::Rejected,
    )));

    assert_eq!(proof_refusal(&host, "granting.dot"), sessionless);
    assert_eq!(proof_refusal(&host, "silent.dot"), sessionless);
    assert_eq!(proof_refusal(&host, &host.product_id()), sessionless);
}

fn proof_refusal(
    host: &ProductRuntimeHost,
    product: &str,
) -> Option<CallError<HostAccountCreateProofError>> {
    futures::executor::block_on(
        host.create_account_proof(&CallContext::default(), create_proof_request(product)),
    )
    .err()
}

#[test]
fn a_proof_naming_the_caller_in_another_spelling_is_still_its_own() {
    // Normalized before comparison, so casing cannot turn a product's own
    // key into a cross-product refusal.
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let shouted = host.product_id().to_uppercase();
    assert_eq!(
        proof_refusal(&host, &shouted),
        Some(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Rejected
        )))
    );
}

#[test]
fn an_unresolvable_product_cannot_reach_a_foreign_key() {
    // A key handle that does not normalize names no product, so it cannot be
    // reached. Note what this asserts: the frontend answers `Unknown { reason }`
    // here, naming the malformed handle, where the authority answers the uniform
    // `NotAllowlisted` for the same input. That asymmetry is real and deliberate
    // at this layer: the id came from this Host's own caller, not off the wire,
    // so telling it that its handle is malformed discloses nothing it did not
    // already send. The authority cannot say the same and does not.
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    assert_eq!(
        proof_refusal(&host, "not a product"),
        Some(CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Unknown {
                reason: "Invalid key handle".to_string()
            }
        )))
    );
}

#[test]
fn get_chain_info_unserved_identifier_is_not_supported() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = RemoteChainInfoRequest::V1(v01::RemoteChainInfoRequest {
        chain: v01::ChainIdentifier::Bulletin,
    });
    let error = futures::executor::block_on(host.get_chain_info(&cx, request)).unwrap_err();
    assert_eq!(
        error,
        CallError::Domain(RemoteChainInfoError::V1(
            v01::RemoteChainInfoError::NotSupported
        ))
    );
}

/// Records which `ChatPlatform` methods the runtime actually reached.
#[derive(Default)]
struct RecordingChatPlatform {
    registered_bots: Mutex<Vec<String>>,
    created_rooms: Mutex<Vec<String>>,
    posted_rooms: Mutex<Vec<String>>,
    posted_payloads: Mutex<Vec<v01::ChatMessageContent>>,
}

#[truapi::async_trait]
impl truapi_platform::ChatPlatform for RecordingChatPlatform {
    async fn create_chat_room(
        &self,
        _product: &ProductContext,
        request: truapi::latest::HostChatCreateRoomRequest,
    ) -> Result<truapi::latest::HostChatCreateRoomResponse, truapi::latest::HostChatCreateRoomError>
    {
        self.created_rooms
            .lock()
            .expect("created rooms mutex poisoned")
            .push(request.room_id);
        Ok(truapi::latest::HostChatCreateRoomResponse {
            status: v01::ChatRoomRegistrationStatus::New,
        })
    }

    async fn register_chat_bot(
        &self,
        _product: &ProductContext,
        request: truapi::latest::HostChatRegisterBotRequest,
    ) -> Result<truapi::latest::HostChatRegisterBotResponse, truapi::latest::HostChatRegisterBotError>
    {
        self.registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .push(request.bot_id);
        Ok(truapi::latest::HostChatRegisterBotResponse {
            status: v01::ChatBotRegistrationStatus::New,
        })
    }

    async fn post_chat_message(
        &self,
        _product: &ProductContext,
        request: truapi::latest::HostChatPostMessageRequest,
    ) -> Result<truapi::latest::HostChatPostMessageResponse, truapi::latest::HostChatPostMessageError>
    {
        self.posted_rooms
            .lock()
            .expect("posted rooms mutex poisoned")
            .push(request.room_id);
        self.posted_payloads
            .lock()
            .expect("posted payloads mutex poisoned")
            .push(request.payload);
        Ok(truapi::latest::HostChatPostMessageResponse {
            message_id: "message-id".to_string(),
        })
    }

    fn subscribe_chat_rooms(
        &self,
        _product: &ProductContext,
    ) -> futures::stream::BoxStream<
        'static,
        Result<truapi::latest::HostChatListSubscribeItem, truapi::latest::GenericError>,
    > {
        Box::pin(futures::stream::empty())
    }
}

#[test]
fn chat_post_message_screens_content_before_it_reaches_a_host() {
    let (host_config, _) = runtime_config("chat.dot");
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        truapi_platform::ProductExecutionKind::Worker,
    )
    .expect("test chat product context is valid");
    let spawner = test_spawner();
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner.clone(),
    );
    let chat_platform = Arc::new(RecordingChatPlatform::default());
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.chat_platform = Some(chat_platform.clone());
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product);
    install_pairing_session(&host, session_info());

    let post = |payload: v01::ChatMessageContent| {
        futures::executor::block_on(Chat::post_message(
            &host,
            &CallContext::default(),
            HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
                room_id: "support".to_string(),
                payload,
            }),
        ))
    };

    // The screen runs at this entrypoint, not only in the helper: a host
    // must never be handed a scheme it would fetch or open.
    let rejected = post(v01::ChatMessageContent::File(v01::ChatFile {
        url: "javascript:alert(document.cookie)".to_string(),
        file_name: "f".to_string(),
        mime_type: "text/plain".to_string(),
        size_bytes: 1,
        text: None,
    }))
    .expect_err("a javascript: file url must not reach the host");
    assert!(matches!(
        rejected,
        CallError::Domain(HostChatPostMessageError::V1(
            v01::HostChatPostMessageError::Unknown { .. }
        ))
    ));

    // A body over budget reports the size variant the protocol declares.
    let too_large = post(v01::ChatMessageContent::Text {
        text: "x".repeat(truapi_platform::CHAT_BODY_MAX_BYTES + 1),
    })
    .expect_err("an over-budget body must not reach the host");
    assert!(matches!(
        too_large,
        CallError::Domain(HostChatPostMessageError::V1(
            v01::HostChatPostMessageError::MessageTooLarge
        ))
    ));

    // The payload arm of the size variant, which the body arm does not cover.
    let big_payload = post(v01::ChatMessageContent::Custom(v01::ChatCustomMessage {
        message_type: "vote".to_string(),
        payload: vec![0; truapi_platform::CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1],
    }))
    .expect_err("an over-budget custom payload must not reach the host");
    assert!(matches!(
        big_payload,
        CallError::Domain(HostChatPostMessageError::V1(
            v01::HostChatPostMessageError::MessageTooLarge
        ))
    ));

    // An over-long room id is not an over-large message.
    let long_room = futures::executor::block_on(Chat::post_message(
        &host,
        &CallContext::default(),
        HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
            room_id: "r".repeat(truapi_platform::CHAT_FIELD_MAX_BYTES + 1),
            payload: v01::ChatMessageContent::Text {
                text: "hi".to_string(),
            },
        }),
    ))
    .expect_err("an over-long room id must be rejected");
    assert!(matches!(
        long_room,
        CallError::Domain(HostChatPostMessageError::V1(
            v01::HostChatPostMessageError::Unknown { .. }
        ))
    ));

    assert!(
        chat_platform
            .posted_rooms
            .lock()
            .expect("posted rooms mutex poisoned")
            .is_empty(),
        "nothing rejected may reach the host"
    );

    // The validated value is what the host receives, not the arriving one:
    // running the screen and discarding its result would pass every
    // rejection assertion above.
    post(v01::ChatMessageContent::Reaction(v01::ChatReaction {
        message_id: "  cafe\u{301}  ".to_string(),
        emoji: "\u{1f3b2}".to_string(),
    }))
    .expect("a normalizable reaction is accepted");
    post(v01::ChatMessageContent::File(v01::ChatFile {
        url: "https://example.invalid".to_string(),
        file_name: "f".to_string(),
        mime_type: "text/plain".to_string(),
        size_bytes: 1,
        text: None,
    }))
    .expect("a resolvable file url is accepted");
    assert_eq!(
        chat_platform
            .posted_payloads
            .lock()
            .expect("posted payloads mutex poisoned")
            .as_slice(),
        &[
            v01::ChatMessageContent::Reaction(v01::ChatReaction {
                message_id: "caf\u{e9}".to_string(),
                emoji: "\u{1f3b2}".to_string(),
            }),
            v01::ChatMessageContent::File(v01::ChatFile {
                url: "https://example.invalid/".to_string(),
                file_name: "f".to_string(),
                mime_type: "text/plain".to_string(),
                size_bytes: 1,
                text: None,
            }),
        ]
    );
}

#[test]
fn chat_room_ids_agree_across_create_and_post() {
    let (host_config, _) = runtime_config("chat.dot");
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        truapi_platform::ProductExecutionKind::Worker,
    )
    .expect("test chat product context is valid");
    let spawner = test_spawner();
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner.clone(),
    );
    let chat_platform = Arc::new(RecordingChatPlatform::default());
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.chat_platform = Some(chat_platform.clone());
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product);
    install_pairing_session(&host, session_info());

    // Precomposed on create, decomposed on post: the host must see one id,
    // or the message lands in a room that does not exist.
    futures::executor::block_on(Chat::create_room(
        &host,
        &CallContext::default(),
        HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
            room_id: "caf\u{e9}".to_string(),
            name: "Cafe".to_string(),
            icon: String::new(),
        }),
    ))
    .expect("create_room accepts a normalizable id");

    futures::executor::block_on(Chat::post_message(
        &host,
        &CallContext::default(),
        HostChatPostMessageRequest::V1(v01::HostChatPostMessageRequest {
            room_id: "cafe\u{301}".to_string(),
            payload: v01::ChatMessageContent::Text {
                text: "hello".to_string(),
            },
        }),
    ))
    .expect("post_message accepts the other spelling of the same id");

    let created = chat_platform
        .created_rooms
        .lock()
        .expect("created rooms mutex poisoned")
        .clone();
    let posted = chat_platform
        .posted_rooms
        .lock()
        .expect("posted rooms mutex poisoned")
        .clone();
    assert_eq!(created, posted);

    // create_room screens the same fields register_bot does.
    for (room_id, icon) in [("", ""), ("room\u{202e}", ""), ("room", "javascript:x")] {
        let rejected = futures::executor::block_on(Chat::create_room(
            &host,
            &CallContext::default(),
            HostChatCreateRoomRequest::V1(v01::HostChatCreateRoomRequest {
                room_id: room_id.to_string(),
                name: "Room".to_string(),
                icon: icon.to_string(),
            }),
        ));
        assert!(
            matches!(
                rejected,
                Err(CallError::Domain(HostChatCreateRoomError::V1(
                    v01::HostChatCreateRoomError::Unknown { .. }
                )))
            ),
            "{room_id:?}/{icon:?} must be a domain error, got {rejected:?}"
        );
    }
}

#[test]
fn chat_register_bot_rejects_unsafe_product_fields() {
    let (host_config, _) = runtime_config("chat.dot");
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        truapi_platform::ProductExecutionKind::Worker,
    )
    .expect("test chat product context is valid");
    let spawner = test_spawner();
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner.clone(),
    );
    let chat_platform = Arc::new(RecordingChatPlatform::default());
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.chat_platform = Some(chat_platform.clone());
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product.clone());
    install_pairing_session(&host, session_info());

    let register = |bot_id: &str, name: &str, icon: &str| {
        futures::executor::block_on(Chat::register_bot(
            &host,
            &CallContext::default(),
            HostChatRegisterBotRequest::V1(v01::HostChatRegisterBotRequest {
                bot_id: bot_id.to_string(),
                name: name.to_string(),
                icon: icon.to_string(),
            }),
        ))
    };

    // A rejected field is a domain error naming the field, not the
    // transport-level `Unsupported` that means "this host has no Chat".
    for (bot_id, name, icon, expected_field) in [
        ("", "Flipper", "", "botId"),
        ("   ", "Flipper", "", "botId"),
        ("flip\u{202e}per", "Flipper", "", "botId"),
        ("flipper", "Flip\u{202e}per", "", "name"),
        ("flipper", "Flipper", "javascript:alert(1)", "icon"),
        ("flipper", "Flipper", "data: text/html,<script>", "icon"),
        ("flipper", "Flipper", "data:image/svg+xml,<svg>", "icon"),
        ("flipper", "Flipper", "file:///etc/passwd", "icon"),
        ("flipper", "Flipper", "//evil.example/x.png", "icon"),
    ] {
        match register(bot_id, name, icon) {
            Err(CallError::Domain(HostChatRegisterBotError::V1(
                v01::HostChatRegisterBotError::Unknown { reason },
            ))) => assert!(
                reason.contains(expected_field),
                "{bot_id:?}/{name:?}/{icon:?} must name {expected_field}, got {reason:?}"
            ),
            other => panic!("{bot_id:?}/{name:?}/{icon:?} must be a domain error: {other:?}"),
        }
    }
    assert!(
        chat_platform
            .registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .is_empty(),
        "no rejected field may reach the host"
    );

    // NFD and NFC spellings normalize to one id, so they cannot become two
    // bots that render identically.
    register("cafe\u{301}", "Cafe", "").expect("normalized id is accepted");
    register("caf\u{e9}", "Cafe", "").expect("normalized id is accepted");
    let bots = chat_platform
        .registered_bots
        .lock()
        .expect("registered bots mutex poisoned");
    assert_eq!(bots.len(), 2);
    assert_eq!(bots[0], bots[1]);
}

/// Guards the failure mode that hid `register_bot`: a `Chat` trait method
/// with no `impl` silently falls back to the trait default and answers
/// `unavailable`, while codegen, the wire table and the TS types all still
/// advertise it.
#[test]
fn chat_register_bot_reaches_the_installed_adapter() {
    let (host_config, _) = runtime_config("chat.dot");
    let product = ProductContext::new_with_execution(
        "chat.dot".to_string(),
        truapi_platform::ProductExecutionKind::Worker,
    )
    .expect("test chat product context is valid");
    let spawner = test_spawner();
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner.clone(),
    );
    let chat_platform = Arc::new(RecordingChatPlatform::default());
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.chat_platform = Some(chat_platform.clone());
    let host = ProductRuntimeHost::from_services(
        services.clone(),
        adapters,
        pairing_host,
        product.clone(),
    );
    install_pairing_session(&host, session_info());

    let response = futures::executor::block_on(Chat::register_bot(
        &host,
        &CallContext::default(),
        HostChatRegisterBotRequest::V1(v01::HostChatRegisterBotRequest {
            bot_id: "flipper".to_string(),
            name: "Flipper".to_string(),
            icon: String::new(),
        }),
    ));

    let HostChatRegisterBotResponse::V1(response) =
        response.expect("register_bot must reach the adapter, not fall back to unavailable");
    assert_eq!(response.status, v01::ChatBotRegistrationStatus::New);
    assert_eq!(
        chat_platform
            .registered_bots
            .lock()
            .expect("registered bots mutex poisoned")
            .as_slice(),
        &["flipper"]
    );
}

/// Records what reaches the host's Pocket adapter and answers from a fixed
/// card set.
struct RecordingPocketPlatform {
    cards: Vec<v01::PocketCard>,
    removed: Mutex<Vec<String>>,
}

impl RecordingPocketPlatform {
    fn with_cards(cards: Vec<(&str, bool)>) -> Self {
        Self {
            cards: cards
                .into_iter()
                .map(|(card_id, privileged)| v01::PocketCard {
                    card_id: card_id.to_string(),
                    privileged,
                })
                .collect(),
            removed: Mutex::new(Vec::new()),
        }
    }
}

#[truapi::async_trait]
impl truapi_platform::PocketPlatform for RecordingPocketPlatform {
    fn subscribe_pocket_cards(
        &self,
        _product: &ProductContext,
    ) -> futures::stream::BoxStream<
        'static,
        Result<truapi::latest::HostPocketListSubscribeItem, truapi::latest::GenericError>,
    > {
        let cards = self.cards.clone();
        Box::pin(futures::stream::iter([
            Ok(truapi::latest::HostPocketListSubscribeItem { cards }),
            Err(truapi::latest::GenericError {
                reason: "host list failed".to_string(),
            }),
        ]))
    }

    async fn remove_pocket_card(
        &self,
        _product: &ProductContext,
        request: truapi::latest::HostPocketRemoveCardRequest,
    ) -> Result<(), truapi::latest::HostPocketRemoveCardError> {
        if self
            .cards
            .iter()
            .any(|card| card.card_id == request.card_id && card.privileged)
        {
            return Err(truapi::latest::HostPocketRemoveCardError::Privileged);
        }
        self.removed
            .lock()
            .expect("removed mutex poisoned")
            .push(request.card_id);
        Ok(())
    }
}

fn pocket_host(
    kind: truapi_platform::ProductExecutionKind,
    pocket: Option<Arc<RecordingPocketPlatform>>,
    with_session: bool,
) -> ProductRuntimeHost {
    let (host_config, _) = runtime_config("pocket.dot");
    let product = ProductContext::new_with_execution("pocket.dot".to_string(), kind)
        .expect("test pocket product context is valid");
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform,
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.pocket_platform =
        pocket.map(|pocket| pocket as Arc<dyn truapi_platform::PocketPlatform>);
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product);
    if with_session {
        install_pairing_session(&host, session_info());
    }
    host
}

fn remove_card(
    host: &ProductRuntimeHost,
    card_id: &str,
) -> Result<HostPocketRemoveCardResponse, CallError<HostPocketRemoveCardError>> {
    futures::executor::block_on(Pocket::remove_card(
        host,
        &CallContext::default(),
        HostPocketRemoveCardRequest::V1(v01::HostPocketRemoveCardRequest {
            card_id: card_id.to_string(),
        }),
    ))
}

/// First thing a Pocket subscription yields: an item, or the interrupt that
/// ended it before any item arrived.
fn first_pocket_item(
    host: &ProductRuntimeHost,
) -> Option<
    Result<
        HostPocketListSubscribeItem,
        CallError<truapi::versioned::pocket::HostPocketListSubscribeError>,
    >,
> {
    futures::executor::block_on(
        futures::executor::block_on(Pocket::list_subscribe(
            host,
            &CallContext::default(),
            truapi::versioned::pocket::HostPocketListSubscribeRequest::V1,
        ))
        .next(),
    )
}

#[test]
fn pocket_list_subscribe_forwards_the_host_list_and_interrupts_on_stream_errors() {
    let pocket = Arc::new(RecordingPocketPlatform::with_cards(vec![
        ("loyalty", false),
        ("humanity", true),
    ]));
    let host = pocket_host(
        truapi_platform::ProductExecutionKind::Worker,
        Some(pocket),
        true,
    );

    let mut items = futures::executor::block_on(Pocket::list_subscribe(
        &host,
        &CallContext::default(),
        truapi::versioned::pocket::HostPocketListSubscribeRequest::V1,
    ));
    let HostPocketListSubscribeItem::V1(first) = futures::executor::block_on(items.next())
        .expect("the host list is forwarded")
        .expect("the host list arrives as an item, not an interrupt");
    assert_eq!(first.cards.len(), 2);
    assert!(
        first
            .cards
            .iter()
            .any(|card| card.card_id == "humanity" && card.privileged)
    );
    // A failing platform stream ends the subscription with an interrupt the
    // product can act on. Dropping it would freeze the product's card list on
    // its last value with nothing to say the host stopped reporting.
    assert!(matches!(
        futures::executor::block_on(items.next()),
        Some(Err(CallError::HostFailure { .. }))
    ));
    assert!(futures::executor::block_on(items.next()).is_none());
}

#[test]
fn pocket_remove_card_normalizes_the_id_and_maps_domain_errors() {
    let pocket = Arc::new(RecordingPocketPlatform::with_cards(vec![
        ("loyalty", false),
        ("humanity", true),
    ]));
    let host = pocket_host(
        truapi_platform::ProductExecutionKind::Worker,
        Some(pocket.clone()),
        true,
    );

    // Trimmed and NFC-normalized before the host sees it, so the host stores
    // one spelling of a card id however the product spells it.
    assert_eq!(
        remove_card(&host, "  loyalty  ").expect("removal succeeds"),
        HostPocketRemoveCardResponse::V1
    );
    assert_eq!(
        pocket
            .removed
            .lock()
            .expect("removed mutex poisoned")
            .as_slice(),
        ["loyalty"]
    );

    assert!(matches!(
        remove_card(&host, "humanity"),
        Err(CallError::Domain(HostPocketRemoveCardError::V1(
            v01::HostPocketRemoveCardError::Privileged
        )))
    ));
    assert!(matches!(
        remove_card(&host, ""),
        Err(CallError::Domain(HostPocketRemoveCardError::V1(
            v01::HostPocketRemoveCardError::Unknown { .. }
        )))
    ));
    assert_eq!(
        pocket.removed.lock().expect("removed mutex poisoned").len(),
        1,
        "a rejected card id never reaches the host"
    );
}

#[test]
fn pocket_is_denied_to_apps_and_sessionless_workers_and_unsupported_without_an_adapter() {
    let pocket = Arc::new(RecordingPocketPlatform::with_cards(vec![]));
    let app = pocket_host(
        truapi_platform::ProductExecutionKind::App,
        Some(pocket.clone()),
        true,
    );
    assert!(matches!(
        remove_card(&app, "loyalty"),
        Err(CallError::Denied)
    ));
    assert!(matches!(
        first_pocket_item(&app),
        Some(Err(CallError::Denied))
    ));

    let no_session = pocket_host(
        truapi_platform::ProductExecutionKind::Worker,
        Some(pocket),
        false,
    );
    assert!(matches!(
        remove_card(&no_session, "loyalty"),
        Err(CallError::Denied)
    ));

    let no_adapter = pocket_host(truapi_platform::ProductExecutionKind::Worker, None, true);
    assert!(matches!(
        remove_card(&no_adapter, "loyalty"),
        Err(CallError::Unsupported)
    ));
    assert!(matches!(
        first_pocket_item(&no_adapter),
        Some(Err(CallError::Unsupported))
    ));
}

/// Records every profile presentation that reaches the host.
#[derive(Default)]
struct RecordingProfilePlatform {
    presented: Mutex<Vec<(String, String)>>,
}

#[truapi::async_trait]
impl truapi_platform::ProfilePlatform for RecordingProfilePlatform {
    async fn present_profile(
        &self,
        product: &ProductContext,
        request: truapi::latest::HostProfilePresentRequest,
    ) -> Result<(), truapi::latest::HostProfilePresentError> {
        self.presented
            .lock()
            .expect("presented mutex poisoned")
            .push((product.product_id.clone(), request.reference));
        Ok(())
    }
}

fn profile_host(profile: Option<Arc<RecordingProfilePlatform>>) -> ProductRuntimeHost {
    let (host_config, product) = runtime_config("egui-chat.dot");
    let services = RuntimeServices::new(
        stub_platform(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.profile_platform =
        profile.map(|profile| profile as Arc<dyn truapi_platform::ProfilePlatform>);
    ProductRuntimeHost::from_services(services, adapters, pairing_host, product)
}

fn present_profile(
    host: &ProductRuntimeHost,
    reference: &str,
) -> Result<HostProfilePresentResponse, CallError<HostProfilePresentError>> {
    futures::executor::block_on(Profile::present(
        host,
        &CallContext::default(),
        HostProfilePresentRequest::V1(v01::HostProfilePresentRequest {
            reference: reference.to_string(),
        }),
    ))
}

#[test]
fn profile_present_forwards_screened_references_and_is_unsupported_without_an_adapter() {
    let profile = Arc::new(RecordingProfilePlatform::default());
    let host = profile_host(Some(profile.clone()));
    let reference = format!("bafkreitest#{}", "ab".repeat(44));
    let longest = "a".repeat(2048);

    assert_eq!(
        present_profile(&host, &reference).expect("a screened reference is presented"),
        HostProfilePresentResponse::V1
    );
    assert_eq!(
        present_profile(&host, &longest).expect("the bound is inclusive"),
        HostProfilePresentResponse::V1
    );
    // Anything that could render deceptively or carry a payload into host UI
    // is refused in the core, whatever the host would have done with it.
    for rejected in [
        String::new(),
        "a".repeat(2049),
        "bafk ref#00".to_string(),
        "bafk\u{202e}ref".to_string(),
        "bafk\nref".to_string(),
    ] {
        assert!(matches!(
            present_profile(&host, &rejected),
            Err(CallError::Domain(HostProfilePresentError::V1(
                v01::HostProfilePresentError::InvalidReference
            )))
        ));
    }
    assert_eq!(
        profile
            .presented
            .lock()
            .expect("presented mutex poisoned")
            .as_slice(),
        [
            ("egui-chat.dot".to_string(), reference),
            ("egui-chat.dot".to_string(), longest),
        ],
        "only screened references reach the host, attributed to the caller"
    );

    assert!(matches!(
        present_profile(&profile_host(None), "bafkreitest#00"),
        Err(CallError::Unsupported)
    ));
}

/// A product runtime on a shared platform, so several products see one core
/// storage the way they do on a real host.
fn profile_host_on(
    platform: Arc<crate::test_support::StubPlatform>,
    product: ProductContext,
    profile: Option<Arc<RecordingProfilePlatform>>,
) -> ProductRuntimeHost {
    let (host_config, _) = runtime_config(&product.product_id);
    let services = RuntimeServices::new(
        platform,
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    adapters.profile_platform =
        profile.map(|profile| profile as Arc<dyn truapi_platform::ProfilePlatform>);
    ProductRuntimeHost::from_services(services, adapters, pairing_host, product)
}

fn disclose(
    host: &ProductRuntimeHost,
    reference: &str,
) -> Result<HostProfileDiscloseResponse, CallError<HostProfileDiscloseError>> {
    futures::executor::block_on(Profile::disclose(
        host,
        &CallContext::default(),
        HostProfileDiscloseRequest::V1(v01::HostProfileDiscloseRequest {
            reference: reference.to_string(),
        }),
    ))
}

fn retract(
    host: &ProductRuntimeHost,
) -> Result<HostProfileRetractResponse, CallError<HostProfileRetractError>> {
    futures::executor::block_on(Profile::retract(
        host,
        &CallContext::default(),
        HostProfileRetractRequest::V1,
    ))
}

fn present_contact(
    host: &ProductRuntimeHost,
    peer_identity: [u8; 32],
) -> Result<HostProfilePresentContactResponse, CallError<HostProfilePresentContactError>> {
    futures::executor::block_on(Profile::present_contact(
        host,
        &CallContext::default(),
        HostProfilePresentContactRequest::V1(v01::HostProfilePresentContactRequest {
            peer_identity,
        }),
    ))
}

const CONTACTS_REFERENCE: &str = "seity-contacts:v1:5c9584ba6e565351723d57394780b31b4c2156123e1269c4724ae5f01258bb535c9584ba6e565351723d57394780b31b4c2156123e1269c4724ae5f01258bb53";

#[test]
fn profile_disclose_stores_the_reference_and_only_its_discloser_may_retract_it() {
    let platform = stub_platform();
    let seity = profile_host_on(
        platform.clone(),
        ProductContext::new("seity.dot".to_string()).expect("valid product"),
        None,
    );
    let other = profile_host_on(
        platform.clone(),
        ProductContext::new("other.dot".to_string()).expect("valid product"),
        None,
    );

    assert_eq!(
        disclose(&seity, CONTACTS_REFERENCE).expect("an App discloses a screened reference"),
        HostProfileDiscloseResponse::V1
    );
    let stored = futures::executor::block_on(profile::read_disclosure(platform.as_ref()))
        .expect("readable")
        .expect("stored");
    assert_eq!(stored.product_id, "seity.dot");
    assert_eq!(stored.reference, CONTACTS_REFERENCE);

    assert!(matches!(
        retract(&other),
        Err(CallError::Domain(HostProfileRetractError::V1(
            v01::HostProfileRetractError::NotDiscloser
        )))
    ));
    assert_eq!(
        retract(&seity).expect("the discloser retracts"),
        HostProfileRetractResponse::V1
    );
    assert_eq!(
        futures::executor::block_on(profile::read_disclosure(platform.as_ref())).expect("readable"),
        None
    );
    assert_eq!(
        retract(&seity).expect("retracting nothing is not an error"),
        HostProfileRetractResponse::V1
    );
}

#[test]
fn profile_disclose_is_for_apps_and_screened_references_only() {
    let platform = stub_platform();
    let worker = profile_host_on(
        platform.clone(),
        ProductContext::new_with_execution(
            "seity.dot".to_string(),
            truapi_platform::ProductExecutionKind::Worker,
        )
        .expect("valid product"),
        None,
    );
    assert!(matches!(
        disclose(&worker, CONTACTS_REFERENCE),
        Err(CallError::Denied)
    ));
    assert!(matches!(retract(&worker), Err(CallError::Denied)));

    let app = profile_host_on(
        platform.clone(),
        ProductContext::new("seity.dot".to_string()).expect("valid product"),
        None,
    );
    for rejected in [
        String::new(),
        "a".repeat(2049),
        "seity contacts".to_string(),
    ] {
        assert!(matches!(
            disclose(&app, &rejected),
            Err(CallError::Domain(HostProfileDiscloseError::V1(
                v01::HostProfileDiscloseError::InvalidReference
            )))
        ));
    }
    assert_eq!(
        futures::executor::block_on(profile::read_disclosure(platform.as_ref())).expect("readable"),
        None,
        "nothing unscreened is stored"
    );
}

#[test]
fn profile_present_contact_substitutes_the_reference_the_contact_sent() {
    let platform = stub_platform();
    let presented = Arc::new(RecordingProfilePlatform::default());
    let chat = profile_host_on(
        platform.clone(),
        ProductContext::new("egui-chat.dot".to_string()).expect("valid product"),
        Some(presented.clone()),
    );
    let alice = [0xa1; 32];
    let bob = [0xb0; 32];
    // What the relay does when Alice's host sends her reference.
    futures::executor::block_on(profile::record_received_reference(
        platform.as_ref(),
        "egui-chat.dot",
        alice,
        "seity.dot".to_string(),
        Some(CONTACTS_REFERENCE.to_string()),
    ))
    .expect("recorded");

    assert_eq!(
        present_contact(&chat, alice).expect("a contact who shared is presented"),
        HostProfilePresentContactResponse::V1
    );
    assert_eq!(
        presented
            .presented
            .lock()
            .expect("presented mutex poisoned")
            .as_slice(),
        [("egui-chat.dot".to_string(), CONTACTS_REFERENCE.to_string())],
        "the host presents the stored reference, attributed to the caller"
    );
    assert!(matches!(
        present_contact(&chat, bob),
        Err(CallError::Domain(HostProfilePresentContactError::V1(
            v01::HostProfilePresentContactError::NotShared
        )))
    ));

    // Another product's contacts are not this product's.
    let other = profile_host_on(
        platform.clone(),
        ProductContext::new("other-chat.dot".to_string()).expect("valid product"),
        Some(presented.clone()),
    );
    assert!(matches!(
        present_contact(&other, alice),
        Err(CallError::Domain(HostProfilePresentContactError::V1(
            v01::HostProfilePresentContactError::NotShared
        )))
    ));

    // A retraction from Alice's host removes what this host holds.
    futures::executor::block_on(profile::record_received_reference(
        platform.as_ref(),
        "egui-chat.dot",
        alice,
        "seity.dot".to_string(),
        None,
    ))
    .expect("recorded");
    assert!(matches!(
        present_contact(&chat, alice),
        Err(CallError::Domain(HostProfilePresentContactError::V1(
            v01::HostProfilePresentContactError::NotShared
        )))
    ));

    assert!(matches!(
        present_contact(
            &profile_host_on(
                platform,
                ProductContext::new("egui-chat.dot".to_string()).expect("valid product"),
                None,
            ),
            alice,
        ),
        Err(CallError::Unsupported)
    ));
}

#[test]
fn chain_follow_ids_are_scoped_per_product_core() {
    let (host_config, product) = runtime_config("same.dot");
    let spawner = test_spawner();
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner.clone(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let first = ProductRuntimeHost::from_services(
        services.clone(),
        crate::host_core::ConnectionAdapters::from_services(&services),
        pairing_host.clone(),
        product.clone(),
    );
    let second = ProductRuntimeHost::from_services(
        services.clone(),
        crate::host_core::ConnectionAdapters::from_services(&services),
        pairing_host,
        product,
    );

    assert_eq!(first.follow_id("request-1"), "c1:request-1");
    assert_eq!(second.follow_id("request-1"), "c2:request-1");
}

#[test]
fn bare_localhost_product_allows_dev_product_accounts() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("localhost"), test_spawner());

    assert!(host.is_product_account_valid_for_caller("myapp.dot"));
}

/// A product destination reaches the platform as a `polkadot://` URL, whatever
/// the product spelled it as. Asserting only that the call succeeded would not
/// notice it arriving as `https://`.
#[test]
fn navigate_to_hands_a_product_destination_over_as_a_polkadot_url() {
    for spelling in [
        "mytestapp.dot",
        "polkadot://mytestapp.dot",
        "https://mytestapp.dot",
    ] {
        let platform = Arc::new(StubPlatform::default());
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::default();
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: spelling.to_string(),
        });

        let response = futures::executor::block_on(host.navigate_to(&cx, request)).unwrap();

        assert_eq!(response, HostNavigateToResponse::V1);
        assert_eq!(
            platform.navigations.lock().unwrap().as_slice(),
            ["polkadot://mytestapp.dot".to_string()],
            "for {spelling}"
        );
    }
}

/// A web address still arrives as `https://`, so the two stay distinguishable.
#[test]
fn navigate_to_hands_a_web_address_over_unchanged() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
        url: "https://example.com/path".to_string(),
    });

    futures::executor::block_on(host.navigate_to(&cx, request)).unwrap();

    assert_eq!(
        platform.navigations.lock().unwrap().as_slice(),
        ["https://example.com/path".to_string()]
    );
}

#[test]
fn permission_prompts_name_the_requesting_product_and_execution_kind() {
    let (host_config, _) = runtime_config("camera.dot");
    let product = ProductContext::new_with_execution(
        "camera.dot".to_string(),
        truapi_platform::ProductExecutionKind::Worker,
    )
    .expect("test product context is valid");
    let spawner = test_spawner();
    let platform = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        spawner,
    );
    let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product.clone());
    let cx = CallContext::default();

    futures::executor::block_on(host.request_device_permission(
        &cx,
        HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera),
    ))
    .expect("device prompt answers");
    futures::executor::block_on(host.request_remote_permission(
        &cx,
        truapi::versioned::permissions::RemotePermissionRequest::V1(v01::RemotePermissionRequest {
            permission: v01::RemotePermission::ChainSubmit,
        }),
    ))
    .expect("remote prompt answers");

    assert_eq!(
        *platform.permission_prompt_products.lock().unwrap(),
        [product.clone(), product],
    );
}

#[test]
fn navigate_to_rejects_invalid_input_without_prompting_or_calling_platform() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    for url in [
        "",
        "javascript:alert(1)",
        "data:text/plain,test",
        "file:///etc/passwd",
        "vbscript:msgbox(1)",
    ] {
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: url.to_string(),
        });
        assert!(matches!(
            futures::executor::block_on(host.navigate_to(&cx, request)),
            Err(CallError::Domain(HostNavigateToError::V1(
                v01::HostNavigateToError::Unknown { .. }
            )))
        ));
    }
    assert_eq!(
        (
            platform.navigations.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
            platform.remote_permission_requests.lock().unwrap().clone(),
        ),
        (vec![], vec![], vec![]),
    );
}

#[test]
fn navigate_to_external_denies_without_open_url_permission() {
    let platform = Arc::new(StubPlatform {
        device_permission_decisions: Mutex::new([truapi_platform::PermissionDecision::Deny].into()),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    let urls = [
        "https://example.com/page",
        "http://other.example.com/page",
        "mailto:someone@example.com",
        "tel:+15551234567",
        "sms:+15551234567",
        "maps:?q=Berlin",
        "polkadot://1exampleaddress",
        "dot:transfer",
    ];
    let mut responses = Vec::new();
    for url in urls {
        responses.push(futures::executor::block_on(host.navigate_to(
            &cx,
            HostNavigateToRequest::V1(v01::HostNavigateToRequest {
                url: url.to_string(),
            }),
        )));
    }
    assert_eq!(
        (
            responses,
            platform.navigations.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
            platform.remote_permission_requests.lock().unwrap().clone(),
        ),
        (
            vec![
                Err(CallError::Domain(HostNavigateToError::V1(
                    v01::HostNavigateToError::PermissionDenied
                )));
                urls.len()
            ],
            vec![],
            vec![v01::HostDevicePermissionRequest::OpenUrl],
            vec![],
        ),
    );
}

#[test]
fn navigate_to_external_reuses_open_url_grant_across_destinations() {
    let platform = Arc::new(StubPlatform {
        remote_permission_denied: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();

    let urls = [
        "https://example.com/first",
        "https://other.example.com/second",
        "http://third.example.com/page",
    ];
    for url in urls {
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: url.to_string(),
        });
        assert_eq!(
            futures::executor::block_on(host.navigate_to(&cx, request)).unwrap(),
            HostNavigateToResponse::V1
        );
    }

    assert_eq!(
        (
            platform.navigations.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
            platform.remote_permission_requests.lock().unwrap().clone(),
        ),
        (
            urls.map(str::to_string).to_vec(),
            vec![v01::HostDevicePermissionRequest::OpenUrl],
            vec![],
        ),
    );
}

#[test]
fn navigate_to_internal_targets_do_not_consume_open_url_permission() {
    let platform = Arc::new(StubPlatform {
        remote_permission_denied: true,
        device_permission_decisions: Mutex::new([truapi_platform::PermissionDecision::Deny].into()),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();

    for url in [
        "mytestapp.dot",
        "localhost:3000",
        "polkadot://mytestapp.dot/-/pocket/open?card=loyalty",
    ] {
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: url.to_string(),
        });
        assert_eq!(
            futures::executor::block_on(host.navigate_to(&cx, request)).unwrap(),
            HostNavigateToResponse::V1,
            "{url} stays within the host's product surface"
        );
    }
    assert_eq!(
        (
            platform.device_permission_requests.lock().unwrap().clone(),
            platform.remote_permission_requests.lock().unwrap().clone(),
        ),
        (vec![], vec![]),
    );
}

#[test]
fn navigate_to_consumes_open_url_allow_once_at_handoff() {
    futures::executor::block_on(async {
        for url in [
            "https://example.com/page",
            "http://other.example.com/page",
            "mailto:someone@example.com",
            "tel:+15551234567",
            "sms:+15551234567",
            "maps:?q=Berlin",
            "polkadot://1exampleaddress",
            "dot:transfer",
        ] {
            for request_upfront in [false, true] {
                let platform = Arc::new(StubPlatform {
                    device_permission_decisions: Mutex::new(
                        [
                            truapi_platform::PermissionDecision::AllowOnce,
                            truapi_platform::PermissionDecision::Deny,
                        ]
                        .into(),
                    ),
                    remote_permission_denied: true,
                    ..Default::default()
                });
                let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
                let cx = CallContext::default();
                if request_upfront {
                    for _ in 0..2 {
                        assert_eq!(
                            host.request_device_permission(
                                &cx,
                                HostDevicePermissionRequest::V1(
                                    v01::HostDevicePermissionRequest::OpenUrl
                                )
                            )
                            .await
                            .unwrap(),
                            HostDevicePermissionResponse::V1(v01::HostDevicePermissionResponse {
                                granted: true
                            }),
                        );
                    }
                }
                let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
                    url: url.to_string(),
                });
                let first = host.navigate_to(&cx, request.clone()).await;
                let status = host
                    .permission_authorization_status(PermissionAuthorizationRequest::Device(
                        v01::HostDevicePermissionRequest::OpenUrl,
                    ))
                    .await
                    .unwrap();
                let second = host.navigate_to(&cx, request).await;
                assert_eq!(
                    (
                        first,
                        second,
                        status,
                        platform.navigations.lock().unwrap().clone(),
                        platform.device_permission_requests.lock().unwrap().clone(),
                        platform.remote_permission_requests.lock().unwrap().clone(),
                    ),
                    (
                        Ok(HostNavigateToResponse::V1),
                        Err(CallError::Domain(HostNavigateToError::V1(
                            v01::HostNavigateToError::PermissionDenied
                        ))),
                        PermissionAuthorizationStatus::NotDetermined,
                        vec![url.to_string()],
                        vec![v01::HostDevicePermissionRequest::OpenUrl; 2],
                        vec![],
                    ),
                    "url={url}, request_upfront={request_upfront}",
                );
            }
        }
    });
}

#[test]
fn device_authorization_consumes_allow_once_for_each_capability() {
    futures::executor::block_on(async {
        for capability in [
            v01::HostDevicePermissionRequest::Camera,
            v01::HostDevicePermissionRequest::Microphone,
        ] {
            for request_upfront in [false, true] {
                let platform = Arc::new(StubPlatform {
                    device_permission_decisions: Mutex::new(
                        [PermissionDecision::AllowOnce, PermissionDecision::Deny].into(),
                    ),
                    ..Default::default()
                });
                let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
                let cx = CallContext::default();
                let request = HostDevicePermissionRequest::V1(capability);
                if request_upfront {
                    for _ in 0..2 {
                        assert_eq!(
                            host.request_device_permission(&cx, request.clone())
                                .await
                                .unwrap(),
                            HostDevicePermissionResponse::V1(v01::HostDevicePermissionResponse {
                                granted: true
                            }),
                        );
                    }
                }
                let first = host.authorize_device_permission(&cx, request.clone()).await;
                let status = host
                    .permission_authorization_status(PermissionAuthorizationRequest::Device(
                        capability,
                    ))
                    .await
                    .unwrap();
                let second = host.authorize_device_permission(&cx, request).await;
                assert_eq!(
                    (
                        first,
                        second,
                        status,
                        platform.device_permission_requests.lock().unwrap().clone()
                    ),
                    (
                        Ok(HostDevicePermissionResponse::V1(
                            v01::HostDevicePermissionResponse { granted: true }
                        )),
                        Ok(HostDevicePermissionResponse::V1(
                            v01::HostDevicePermissionResponse { granted: false }
                        )),
                        PermissionAuthorizationStatus::NotDetermined,
                        vec![capability; 2],
                    ),
                    "capability={capability}, request_upfront={request_upfront}",
                );
            }
        }
    });
}

#[test]
fn device_authorization_concurrent_calls_cannot_share_one_use_grants() {
    futures::executor::block_on(async {
        for capability in [
            v01::HostDevicePermissionRequest::Camera,
            v01::HostDevicePermissionRequest::Microphone,
        ] {
            let platform = Arc::new(StubPlatform {
                device_permission_decisions: Mutex::new(
                    [PermissionDecision::AllowOnce, PermissionDecision::Deny].into(),
                ),
                ..Default::default()
            });
            let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
            let cx = CallContext::default();
            let request = HostDevicePermissionRequest::V1(capability);
            host.request_device_permission(&cx, request.clone())
                .await
                .unwrap();
            let (first, second) = futures::join!(
                host.authorize_device_permission(&cx, request.clone()),
                host.authorize_device_permission(&cx, request),
            );
            assert_eq!(
                (
                    first,
                    second,
                    platform.device_permission_requests.lock().unwrap().clone()
                ),
                (
                    Ok(HostDevicePermissionResponse::V1(
                        v01::HostDevicePermissionResponse { granted: true }
                    )),
                    Ok(HostDevicePermissionResponse::V1(
                        v01::HostDevicePermissionResponse { granted: false }
                    )),
                    vec![capability; 2],
                ),
                "capability={capability}",
            );
        }
    });
}

#[test]
fn device_authorization_reuses_permanent_grants() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform {
            device_permission_decisions: Mutex::new(
                [
                    PermissionDecision::AllowAlways,
                    PermissionDecision::AllowAlways,
                    PermissionDecision::Deny,
                ]
                .into(),
            ),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
        let capabilities = [
            v01::HostDevicePermissionRequest::Camera,
            v01::HostDevicePermissionRequest::Microphone,
        ];
        let mut responses = Vec::new();
        for _ in 0..2 {
            for capability in capabilities {
                responses.push(
                    host.authorize_device_permission(
                        &CallContext::default(),
                        HostDevicePermissionRequest::V1(capability),
                    )
                    .await,
                );
            }
        }
        assert_eq!(
            (
                responses,
                platform.device_permission_requests.lock().unwrap().clone()
            ),
            (
                vec![
                    Ok(HostDevicePermissionResponse::V1(
                        v01::HostDevicePermissionResponse { granted: true }
                    ));
                    4
                ],
                capabilities.to_vec(),
            ),
        );
    });
}

#[test]
fn device_authorization_storage_failure_does_not_prompt_or_authorize() {
    let platform = Arc::new(StubPlatform {
        permission_storage_error: Some("device permission storage unavailable"),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let response = futures::executor::block_on(host.authorize_device_permission(
        &CallContext::default(),
        HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera),
    ));
    assert_eq!(
        (response, platform.device_permission_requests.lock().unwrap().clone()),
        (
            Err(CallError::HostFailure {
                reason: "permission storage failed: GenericError { reason: \"device permission storage unavailable\" }".to_string(),
            }),
            vec![],
        ),
    );
}

#[test]
fn push_notification_delegates_payload_and_returns_host_id() {
    let pushed_notifications = Arc::new(Mutex::new(Vec::new()));
    let platform = Arc::new(StubPlatform {
        notification_id: 42,
        pushed_notifications: pushed_notifications.clone(),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());
    let cx = CallContext::default();
    let request = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
        text: "Hello".to_string(),
        deeplink: Some("https://example.invalid/launch".to_string()),
        scheduled_at: Some(1_776_144_000_000),
    });

    let response = futures::executor::block_on(host.send_push_notification(&cx, request)).unwrap();

    assert_eq!(
        response,
        HostPushNotificationResponse::V1(v01::HostPushNotificationResponse { id: 42 })
    );
    assert_eq!(
        pushed_notifications
            .lock()
            .expect("notification list mutex poisoned")
            .as_slice(),
        &[v01::HostPushNotificationRequest {
            text: "Hello".to_string(),
            deeplink: Some("https://example.invalid/launch".to_string()),
            scheduled_at: Some(1_776_144_000_000),
        }]
    );
}

#[test]
fn push_notification_consumes_allow_once_before_scheduling() {
    for request_upfront in [false, true] {
        let platform = Arc::new(StubPlatform {
            device_permission_decisions: Mutex::new(
                [
                    truapi_platform::PermissionDecision::AllowOnce,
                    truapi_platform::PermissionDecision::Deny,
                ]
                .into(),
            ),
            notification_id: 42,
            ..Default::default()
        });
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::default();
        let notification = v01::HostPushNotificationRequest {
            text: "Hello".to_string(),
            deeplink: None,
            scheduled_at: None,
        };

        if request_upfront {
            assert_eq!(
                futures::executor::block_on(host.request_device_permission(
                    &cx,
                    HostDevicePermissionRequest::V1(
                        v01::HostDevicePermissionRequest::Notifications
                    ),
                ))
                .unwrap(),
                HostDevicePermissionResponse::V1(v01::HostDevicePermissionResponse {
                    granted: true
                })
            );
        }
        let first = futures::executor::block_on(
            host.send_push_notification(&cx, HostPushNotificationRequest::V1(notification.clone())),
        );
        let second = futures::executor::block_on(
            host.send_push_notification(&cx, HostPushNotificationRequest::V1(notification.clone())),
        );

        assert_eq!(
            (
                first,
                second,
                platform.pushed_notifications.lock().unwrap().clone(),
                platform.device_permission_requests.lock().unwrap().clone(),
            ),
            (
                Ok(HostPushNotificationResponse::V1(
                    v01::HostPushNotificationResponse { id: 42 }
                )),
                Err(CallError::Domain(HostPushNotificationError::V1(
                    v01::HostPushNotificationError::Unknown {
                        reason: PERMISSION_DENIED_REASON.to_string(),
                    },
                ))),
                vec![notification],
                vec![v01::HostDevicePermissionRequest::Notifications; 2],
            ),
            "request_upfront={request_upfront}"
        );
    }
}

#[test]
fn push_notification_denial_never_reaches_scheduler() {
    let platform = Arc::new(StubPlatform {
        device_permission_decisions: Mutex::new([truapi_platform::PermissionDecision::Deny].into()),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    let request = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
        text: "Hello".to_string(),
        deeplink: None,
        scheduled_at: None,
    });

    let response = futures::executor::block_on(host.send_push_notification(&cx, request));

    assert_eq!(
        (
            response,
            platform.pushed_notifications.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
        ),
        (
            Err(CallError::Domain(HostPushNotificationError::V1(
                v01::HostPushNotificationError::Unknown {
                    reason: PERMISSION_DENIED_REASON.to_string(),
                },
            ))),
            vec![],
            vec![v01::HostDevicePermissionRequest::Notifications],
        )
    );
}

#[test]
fn push_notification_reuses_allow_always() {
    let platform = Arc::new(StubPlatform {
        device_permission_decisions: Mutex::new(
            [
                truapi_platform::PermissionDecision::AllowAlways,
                truapi_platform::PermissionDecision::Deny,
            ]
            .into(),
        ),
        notification_id: 42,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    let notification = v01::HostPushNotificationRequest {
        text: "Hello".to_string(),
        deeplink: None,
        scheduled_at: None,
    };
    let first = futures::executor::block_on(
        host.send_push_notification(&cx, HostPushNotificationRequest::V1(notification.clone())),
    );
    let second = futures::executor::block_on(
        host.send_push_notification(&cx, HostPushNotificationRequest::V1(notification.clone())),
    );

    assert_eq!(
        (
            first,
            second,
            platform.pushed_notifications.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
        ),
        (
            Ok(HostPushNotificationResponse::V1(
                v01::HostPushNotificationResponse { id: 42 }
            )),
            Ok(HostPushNotificationResponse::V1(
                v01::HostPushNotificationResponse { id: 42 }
            )),
            vec![notification; 2],
            vec![v01::HostDevicePermissionRequest::Notifications],
        )
    );
}

#[test]
fn cancel_notification_delegates_host_id() {
    let cancelled_notifications = Arc::new(Mutex::new(Vec::new()));
    let platform = Arc::new(StubPlatform {
        cancelled_notifications: cancelled_notifications.clone(),
        device_permission_decisions: Mutex::new([truapi_platform::PermissionDecision::Deny].into()),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let cx = CallContext::default();
    let request =
        HostPushNotificationCancelRequest::V1(v01::HostPushNotificationCancelRequest { id: 42 });

    let response =
        futures::executor::block_on(host.cancel_push_notification(&cx, request)).unwrap();

    assert_eq!(
        (
            response,
            cancelled_notifications.lock().unwrap().clone(),
            platform.device_permission_requests.lock().unwrap().clone(),
        ),
        (HostPushNotificationCancelResponse::V1, vec![42], vec![])
    );
}

#[test]
fn get_account_requires_session() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetError::V1(
            v01::HostAccountGetError::NotConnected
        ))
    ));
}

#[test]
fn get_account_maps_subtree_disconnect_race_to_not_connected() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_peer_disconnect_response_script(&session)),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
    host.test_session_state().set_session(session);
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: account_id("myapp.dot", 0),
    });

    let err = futures::executor::block_on(host.get_account(&CallContext::default(), request))
        .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetError::V1(
            v01::HostAccountGetError::NotConnected
        ))
    ));
}

#[test]
fn get_account_rejects_invalid_product_identifier() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "example.com".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetError::V1(
            v01::HostAccountGetError::DomainNotValid
        ))
    ));
}

#[test]
fn get_account_other_product_rejects_when_user_declines() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "other.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::Rejected))
    ));
    assert_eq!(
        platform
            .account_access_reviews
            .lock()
            .expect("account access review list mutex poisoned")
            .as_slice(),
        &[AccountAccessReview {
            requesting_product_id: "myapp.dot".to_string(),
            target_product_id: "other.dot".to_string(),
        }]
    );
}

#[test]
fn get_account_other_product_maps_confirmation_failure_to_host_failure() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            account_access_error: Some("modal failed"),
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "other.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
    assert!(matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed")));
}

#[test]
fn get_account_other_product_accepts_confirmation_then_derives_key() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            account_access_confirmed: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let session = sso_session_info();
    install_pairing_session(&host, session.clone());
    cache_test_product_subtree(&host, &session, "other.dot");
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "other.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
    let HostAccountGetResponse::V1(inner) = response;
    assert_eq!(
        inner.account.public_key,
        test_product_account_public("other.dot", 0).to_vec()
    );
}

#[test]
fn get_account_allow_once_does_not_authorize_the_next_disclosure() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform {
            permission_confirmation_decisions: Mutex::new(
                [
                    truapi_platform::PermissionDecision::AllowOnce,
                    truapi_platform::PermissionDecision::Deny,
                ]
                .into(),
            ),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let session = sso_session_info();
        install_pairing_session(&host, session.clone());
        cache_test_product_subtree(&host, &session, "other.dot");
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: account_id("other.dot", 0),
        });
        let response = host
            .get_account(&CallContext::default(), request.clone())
            .await
            .unwrap();
        let HostAccountGetResponse::V1(response) = response;
        let saved = platform
            .read_core_storage(CoreStorageKey::account_access_authorization(
                "myapp", "other",
            ))
            .await
            .unwrap();
        let mut rejected = Vec::new();
        for _ in 0..2 {
            rejected.push(matches!(
                host.get_account(&CallContext::default(), request.clone())
                    .await,
                Err(CallError::Domain(HostAccountGetError::V1(
                    v01::HostAccountGetError::Rejected
                )))
            ));
        }
        assert_eq!(
            (
                response.account.public_key,
                saved,
                rejected,
                platform.account_access_reviews.lock().unwrap().len(),
            ),
            (
                test_product_account_public("other.dot", 0).to_vec(),
                None,
                vec![true, true],
                2,
            ),
        );
    });
}

#[test]
fn get_account_derives_rfc0022_product_key() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
    let HostAccountGetResponse::V1(inner) = response;
    assert_eq!(
        hex::encode(inner.account.public_key),
        "1c1ae478b564572f806ffa6352b4273d612beb01610b19f4e5bf444521cd5b5c"
    );
}

#[test]
fn get_account_answers_when_host_storage_never_returns() {
    // A host whose storage never answers must not be able to park the request.
    // Resolving an own-product account reads the persisted subtree first, and
    // that read is a host callback: without a deadline the call never settles,
    // so the product waits forever with no response and no error frame
    // (paritytech/dotli-community#271).
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            core_storage_pending: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    host.test_session_state().set_session(sso_session_info());
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: account_id("myapp.dot", 0),
    });
    // The caller's deadline is what bounds it; a short one keeps the test fast
    // and proves the bound is the context's rather than a fixed sleep.
    let mut cx = CallContext::default();
    cx.set_timeout(std::time::Duration::from_millis(200));

    let outcome = futures::executor::block_on(async {
        use futures::FutureExt;
        use futures::pin_mut;
        let call = host.get_account(&cx, request).fuse();
        let guard = futures_timer::Delay::new(std::time::Duration::from_secs(10)).fuse();
        pin_mut!(call, guard);
        futures::select! {
            result = call => Some(result),
            () = guard => None,
        }
    });

    let Some(result) = outcome else {
        panic!("get_account never answered a host whose storage does not return");
    };
    assert!(
        result.is_err(),
        "a host that cannot answer its own storage must not resolve an account"
    );
}

#[test]
fn get_account_own_product_prompts_and_rejects_on_a_cold_subtree() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            product_subtree_denied: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    // Session without a cached subtree: resolving it must reach the
    // Account Holder, which is the one point the consent prompt fires.
    host.test_session_state().set_session(sso_session_info());
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: account_id("myapp.dot", 0),
    });

    let err = futures::executor::block_on(host.get_account(&CallContext::default(), request))
        .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::Rejected))
    ));
}

#[test]
fn get_account_own_product_skips_the_prompt_when_the_subtree_is_cached() {
    // denied would reject if the prompt fired; a warm cache must not prompt.
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            product_subtree_denied: true,
            ..Default::default()
        }),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: account_id("myapp.dot", 0),
    });

    let response = futures::executor::block_on(host.get_account(&CallContext::default(), request))
        .expect("a cached subtree resolves without prompting");
    let HostAccountGetResponse::V1(inner) = response;
    assert_eq!(
        inner.account.public_key,
        test_product_account_public("myapp.dot", 0).to_vec()
    );
}

#[test]
fn remote_authority_call_bounds_a_call_that_never_unwinds() {
    let mut cx = CallContext::default();
    cx.set_timeout(Duration::from_millis(1));
    // A call that never completes and never observes the token, standing in
    // for one parked in the un-cancellable statement-store setup. The old
    // code awaited it after cancelling and hung; it must now be dropped at
    // the grace and return a bounded timeout.
    let call = futures::future::pending::<Result<(), AuthorityError>>();

    let err = futures::executor::block_on(remote_authority_call(&cx, call))
        .expect_err("a never-unwinding call is bounded by the deadline plus grace");

    assert!(matches!(err, AuthorityError::Cancelled(_)));
}

#[test]
fn get_account_normalizes_product_identifier_before_deriving() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("MyApp.DOT"), test_spawner());
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "MyApp.DOT".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
    let HostAccountGetResponse::V1(inner) = response;
    assert_eq!(
        hex::encode(inner.account.public_key),
        "1c1ae478b564572f806ffa6352b4273d612beb01610b19f4e5bf444521cd5b5c"
    );
}

#[test]
fn get_account_localhost_product_prompts_for_other_product_identifier() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform {
            account_access_confirmed: true,
            ..Default::default()
        }),
        runtime_config("localhost:3000"),
        test_spawner(),
    );
    let session = sso_session_info();
    install_pairing_session(&host, session.clone());
    cache_test_product_subtree(&host, &session, "myapp.dot");
    let cx = CallContext::default();
    let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        },
    });
    let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
    let HostAccountGetResponse::V1(inner) = response;
    assert_eq!(
        hex::encode(inner.account.public_key),
        "1c1ae478b564572f806ffa6352b4273d612beb01610b19f4e5bf444521cd5b5c"
    );
}

#[test]
fn get_account_alias_requires_session() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    let cx = CallContext::default();
    let err = futures::executor::block_on(
        host.get_account_alias(&cx, account_alias_request("myapp.dot")),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountGetAliasError::V1(
            v01::HostAccountGetAliasError::Rejected
        ))
    ));
}

#[test]
fn get_account_alias_forwards_without_pairing_host_confirmation() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-alias-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::GetAccountAliasResponse(Response {
                    responding_to: "alias-1".to_string(),
                    payload: Ok(v01::ContextualAlias {
                        context: [9; 32],
                        alias: vec![1, 2, 3],
                    }),
                })),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("alias-1".to_string());
    let response = futures::executor::block_on(
        host.get_account_alias(&cx, account_alias_request("myapp.dot")),
    )
    .unwrap();
    let HostAccountGetAliasResponse::V1(inner) = response;
    assert_eq!(inner.context, [9; 32]);
    assert_eq!(inner.alias, vec![1, 2, 3]);
    let message = submitted_remote_message(&platform, &session);
    let RemoteMessageData::V1(v1::RemoteMessage::GetAccountAliasRequest(request)) = message.data
    else {
        panic!("expected ring VRF alias request");
    };
    assert_eq!(request.calling_product_id, "myapp.dot");
    assert_eq!(request.payload.context.product_id, "myapp.dot");
    assert_eq!(request.payload.ring_location.chain_id, [1; 32]);
}

#[test]
fn create_account_proof_requires_session() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    let cx = CallContext::default();
    let err = futures::executor::block_on(
        host.create_account_proof(&cx, create_proof_request("myapp.dot")),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::Rejected
        ))
    ));
}

#[test]
fn create_account_proof_returns_sso_proof() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-proof-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::CreateAccountProofResponse(
                    Response {
                        responding_to: "proof-1".to_string(),
                        payload: Ok(v01::HostAccountCreateProofResponse {
                            proof: vec![0xaa, 0xbb],
                            contextual_alias: v01::ContextualAlias {
                                context: [9; 32],
                                alias: vec![1, 2, 3],
                            },
                            ring_index: 5,
                            ring_revision: 7,
                        }),
                    },
                )),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("proof-1".to_string());
    let response = futures::executor::block_on(
        host.create_account_proof(&cx, create_proof_request("myapp.dot")),
    )
    .unwrap();
    let HostAccountCreateProofResponse::V1(inner) = response;
    assert_eq!(inner.proof, vec![0xaa, 0xbb]);
    assert_eq!(inner.ring_index, 5);
    assert_eq!(inner.ring_revision, 7);
    let message = submitted_remote_message(&platform, &session);
    let RemoteMessageData::V1(v1::RemoteMessage::CreateAccountProofRequest(request)) = message.data
    else {
        panic!("expected ring VRF proof request");
    };
    assert_eq!(request.calling_product_id, "myapp.dot");
    assert_eq!(request.payload.context.product_id, "myapp.dot");
    assert_eq!(request.payload.message, vec![4, 5, 6]);
}

#[test]
fn create_account_proof_maps_not_member_error() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-proof-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::CreateAccountProofResponse(
                    Response {
                        responding_to: "proof-1".to_string(),
                        payload: Err(RingVrfError::NotMember),
                    },
                )),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);
    let cx = CallContext::with_request_id("proof-1".to_string());
    let err = futures::executor::block_on(
        host.create_account_proof(&cx, create_proof_request("myapp.dot")),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CallError::Domain(HostAccountCreateProofError::V1(
            v01::HostAccountCreateProofError::NotMember
        ))
    ));
}

#[test]
fn get_legacy_accounts_returns_empty_when_connected() {
    let host = ProductRuntimeHost::new(
        stub_platform(),
        runtime_config("localhost:3000"),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let response = futures::executor::block_on(
        host.get_legacy_accounts(&cx, HostGetLegacyAccountsRequest::V1),
    )
    .unwrap();
    let HostGetLegacyAccountsResponse::V1(inner) = response;
    assert!(inner.accounts.is_empty());
}

#[test]
fn get_legacy_accounts_returns_empty_when_disconnected() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let response = futures::executor::block_on(
        host.get_legacy_accounts(&cx, HostGetLegacyAccountsRequest::V1),
    )
    .unwrap();
    let HostGetLegacyAccountsResponse::V1(inner) = response;
    assert!(inner.accounts.is_empty());
}

#[test]
fn get_user_id_returns_primary_username() {
    let platform = Arc::new(StubPlatform {
        identity_disclosure_confirmed: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let response =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();
    let HostGetUserIdResponse::V1(inner) = response;
    assert_eq!(inner.primary_username, "Alice Smith");
    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn get_user_id_caches_identity_disclosure_grant() {
    let platform = Arc::new(StubPlatform {
        identity_disclosure_confirmed: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();

    futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();
    futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();

    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 1);
    let status = futures::executor::block_on(
        host.permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure),
    )
    .unwrap();
    assert_eq!(status, PermissionAuthorizationStatus::Authorized);
}

#[test]
fn get_user_id_allow_once_does_not_authorize_the_next_disclosure() {
    futures::executor::block_on(async {
        let platform = Arc::new(StubPlatform {
            permission_confirmation_decisions: Mutex::new(
                [
                    truapi_platform::PermissionDecision::AllowOnce,
                    truapi_platform::PermissionDecision::Deny,
                ]
                .into(),
            ),
            ..Default::default()
        });
        let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
        install_pairing_session(&host, session_info());
        let response = host
            .get_user_id(&CallContext::default(), HostGetUserIdRequest::V1)
            .await
            .unwrap();
        let HostGetUserIdResponse::V1(response) = response;
        let status = host
            .permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure)
            .await
            .unwrap();
        let mut rejected = Vec::new();
        for _ in 0..2 {
            rejected.push(matches!(
                host.get_user_id(&CallContext::default(), HostGetUserIdRequest::V1)
                    .await,
                Err(CallError::Domain(HostGetUserIdError::V1(
                    v01::HostGetUserIdError::PermissionDenied
                )))
            ));
        }
        assert_eq!(
            (
                response.primary_username,
                status,
                rejected,
                platform.identity_disclosure_calls.load(Ordering::SeqCst),
            ),
            (
                "Alice Smith".to_string(),
                PermissionAuthorizationStatus::NotDetermined,
                vec![true, true],
                2,
            ),
        );
    });
}

#[test]
fn get_user_id_caches_identity_disclosure_denial() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();

    let first =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();
    let second =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();

    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        first,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::PermissionDenied
        ))
    ));
    assert!(matches!(
        second,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::PermissionDenied
        ))
    ));
    let status = futures::executor::block_on(
        host.permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure),
    )
    .unwrap();
    assert_eq!(status, PermissionAuthorizationStatus::Denied);
}

#[test]
fn get_user_id_dismissed_identity_disclosure_stays_not_determined() {
    let platform = Arc::new(StubPlatform {
        identity_disclosure_error: Some("dismissed"),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();

    let first =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();
    let second =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();

    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 2);
    assert!(matches!(
        first,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::PermissionDenied
        ))
    ));
    assert!(matches!(
        second,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::PermissionDenied
        ))
    ));
    let status = futures::executor::block_on(
        host.permission_authorization_status(PermissionAuthorizationRequest::IdentityDisclosure),
    )
    .unwrap();
    assert_eq!(status, PermissionAuthorizationStatus::NotDetermined);
}

#[test]
fn get_user_id_checks_identity_disclosure_before_username() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let mut session = session_info();
    session.full_username = None;
    session.lite_username = None;
    install_pairing_session(&host, session);
    let cx = CallContext::default();

    let err =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::PermissionDenied
        ))
    ));
    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn get_user_id_reports_missing_username_after_identity_disclosure() {
    let platform = Arc::new(StubPlatform {
        identity_disclosure_confirmed: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let mut session = session_info();
    session.full_username = None;
    session.lite_username = None;
    install_pairing_session(&host, session);
    let cx = CallContext::default();

    let err =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostGetUserIdError::V1(
            v01::HostGetUserIdError::Unknown { ref reason }
        )) if reason == "No primary username for this session"
    ));
    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn get_user_id_respects_pre_authorized_identity_disclosure() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    futures::executor::block_on(host.set_permission_authorization_status(
        PermissionAuthorizationRequest::IdentityDisclosure,
        PermissionAuthorizationStatus::Authorized,
    ))
    .unwrap();

    let response =
        futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();
    let HostGetUserIdResponse::V1(inner) = response;
    assert_eq!(inner.primary_username, "Alice Smith");
    assert_eq!(platform.identity_disclosure_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn derive_entropy_matches_dotli_vector() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    let mut session = sso_session_info();
    session.root_entropy_source = session_info().root_entropy_source;
    install_pairing_session(&host, session);
    let cx = CallContext::default();
    let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
        context: b"product-key".to_vec(),
    });
    let response = futures::executor::block_on(host.derive(&cx, request)).unwrap();
    let HostDeriveEntropyResponse::V1(inner) = response;
    assert_eq!(
        hex::encode(inner.entropy),
        "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36"
    );
}

#[test]
fn derive_entropy_requires_session() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
        context: b"product-key".to_vec(),
    });
    let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
    match err {
        CallError::Domain(HostDeriveEntropyError::V1(v01::HostDeriveEntropyError::Unknown {
            reason,
        })) => assert_eq!(reason, "Not connected"),
        other => panic!("expected Unknown entropy error, got {other:?}"),
    }
}

#[test]
fn derive_entropy_requires_secret() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let mut session = sso_session_info();
    session.root_entropy_source = None;
    install_pairing_session(&host, session);
    let cx = CallContext::default();
    let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
        context: b"product-key".to_vec(),
    });
    let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
    match err {
        CallError::Domain(HostDeriveEntropyError::V1(v01::HostDeriveEntropyError::Unknown {
            reason,
        })) => assert_eq!(reason, "Session secret missing"),
        other => panic!("expected Unknown entropy error, got {other:?}"),
    }
}

#[test]
fn derive_entropy_rejects_empty_context_like_dotli_key() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let mut session = sso_session_info();
    session.root_entropy_source = session_info().root_entropy_source;
    install_pairing_session(&host, session);
    let cx = CallContext::default();
    let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context: vec![] });
    let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
    match err {
        CallError::Domain(HostDeriveEntropyError::V1(v01::HostDeriveEntropyError::Unknown {
            reason,
        })) => assert_eq!(reason, "\"key\" must be between 1 and 32 bytes, got 0"),
        other => panic!("expected Unknown entropy error, got {other:?}"),
    }
}

#[test]
fn preimage_submit_requires_session_first() {
    let host = ProductRuntimeHost::new_compat_with_bulletin(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = RemotePreimageSubmitRequest::V1(vec![1, 2, 3]);

    let err = futures::executor::block_on(Preimage::submit(&host, &cx, request)).unwrap_err();

    match err {
        CallError::Domain(RemotePreimageSubmitError::V1(v01::PreimageSubmitError::Unknown {
            reason,
        })) => assert_eq!(reason, "No active session"),
        other => panic!("expected preimage session error, got {other:?}"),
    }
}

#[test]
fn preimage_submit_requires_remote_permission_before_backend_call() {
    let platform = Arc::new(StubPlatform {
        remote_permission_denied: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat_with_bulletin(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let request = RemotePreimageSubmitRequest::V1(vec![1, 2, 3]);
    let err = futures::executor::block_on(Preimage::submit(&host, &cx, request)).unwrap_err();
    match err {
        CallError::Domain(RemotePreimageSubmitError::V1(v01::PreimageSubmitError::Unknown {
            reason,
        })) => assert_eq!(reason, PERMISSION_DENIED_REASON),
        other => panic!("expected preimage permission denial, got {other:?}"),
    }
    assert!(
        platform
            .sent_rpc
            .lock()
            .expect("rpc list mutex poisoned")
            .is_empty()
    );
}

#[test]
fn chain_broadcast_requires_remote_permission_before_backend_call() {
    let platform = Arc::new(StubPlatform {
        remote_permission_denied: true,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();
    let request =
        RemoteChainTransactionBroadcastRequest::V1(v01::RemoteChainTransactionBroadcastRequest {
            genesis_hash: vec![0; 32],
            transaction: vec![1, 2, 3],
        });
    let err =
        futures::executor::block_on(Chain::broadcast_transaction(&host, &cx, request)).unwrap_err();
    match err {
        CallError::Domain(RemoteChainTransactionBroadcastError::V1(v01::GenericError {
            reason,
        })) => assert_eq!(reason, PERMISSION_DENIED_REASON),
        other => panic!("expected chain broadcast permission denial, got {other:?}"),
    }
    assert!(platform.sent_rpc.lock().unwrap().is_empty());
}

#[test]
fn preimage_lookup_cache_hit_emits_once_and_stays_open() {
    use futures::FutureExt;

    use crate::host_logic::bulletin::preimage_key;

    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let value = vec![4, 5, 6, 7];
    let key = preimage_key(&value);
    host.services.cache_preimage(key, value.clone());

    let cx = CallContext::default();
    let request =
        RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key: key.to_vec(),
        });
    let mut subscription = futures::executor::block_on(host.lookup_subscribe(&cx, request));
    let item = futures::executor::block_on(subscription.next()).expect("preimage item");
    assert_eq!(
        item,
        Ok(RemotePreimageLookupSubscribeItem::V1(
            v01::RemotePreimageLookupSubscribeItem { value: Some(value) }
        ))
    );
    // The subscription stays open (no completion/interrupt frame) after the
    // single cache-hit emission.
    assert!(subscription.next().now_or_never().is_none());
}

#[test]
fn preimage_lookup_forged_host_bytes_downgraded_to_miss() {
    use crate::host_logic::bulletin::preimage_key;

    let value = vec![1, 1, 2, 3, 5, 8];
    let key = preimage_key(&value);

    // Host returns bytes that do not hash to the requested key.
    let forged = Arc::new(StubPlatform {
        preimage_lookup_value: Some(vec![9, 9, 9]),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(forged, test_spawner());
    let cx = CallContext::default();
    let request =
        RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key: key.to_vec(),
        });
    let mut subscription = futures::executor::block_on(host.lookup_subscribe(&cx, request));
    let item = futures::executor::block_on(subscription.next()).expect("preimage item");
    assert_eq!(
        item,
        Ok(RemotePreimageLookupSubscribeItem::V1(
            v01::RemotePreimageLookupSubscribeItem { value: None }
        ))
    );

    // Correct bytes pass the integrity check through.
    let genuine = Arc::new(StubPlatform {
        preimage_lookup_value: Some(value.clone()),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(genuine, test_spawner());
    let request =
        RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key: key.to_vec(),
        });
    let mut subscription = futures::executor::block_on(host.lookup_subscribe(&cx, request));
    let item = futures::executor::block_on(subscription.next()).expect("preimage item");
    assert_eq!(
        item,
        Ok(RemotePreimageLookupSubscribeItem::V1(
            v01::RemotePreimageLookupSubscribeItem { value: Some(value) }
        ))
    );
}

fn storage_item(
    value: Option<&[u8]>,
) -> Result<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
    Ok(HostLocalStorageChangeItem::V1(
        v01::HostLocalStorageChangeItem {
            value: value.map(<[u8]>::to_vec),
        },
    ))
}

fn subscribe_storage_key(
    host: &ProductRuntimeHost,
    key: &str,
) -> Subscription<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
    futures::executor::block_on(LocalStorage::subscribe(
        host,
        &CallContext::default(),
        HostLocalStorageSubscribeRequest::V1(v01::HostLocalStorageSubscribeRequest {
            key: key.to_string(),
        }),
    ))
}

fn write_storage_key(host: &ProductRuntimeHost, key: &str, value: &[u8]) {
    futures::executor::block_on(host.write(
        &CallContext::default(),
        HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest {
            key: key.to_string(),
            value: value.to_vec(),
        }),
    ))
    .expect("storage write");
}

#[test]
fn local_storage_subscribe_sees_writes_and_clears_but_not_identical_rewrites() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let mut subscription = subscribe_storage_key(&host, "progress");
    let next = |subscription: &mut Subscription<
        HostLocalStorageChangeItem,
        CallError<HostLocalStorageSubscribeError>,
    >| { futures::executor::block_on(subscription.next()).expect("storage item") };

    assert_eq!(
        next(&mut subscription),
        storage_item(None),
        "the first item is the current value"
    );

    write_storage_key(&host, "progress", b"1");
    assert_eq!(next(&mut subscription), storage_item(Some(b"1")));

    write_storage_key(&host, "progress", b"1");
    assert!(
        futures::FutureExt::now_or_never(subscription.next()).is_none(),
        "a byte-identical rewrite emits nothing"
    );
    assert_eq!(
        platform
            .local_storage_writes
            .lock()
            .expect("local storage writes mutex poisoned")
            .len(),
        2,
        "every write reaches the platform, so a host hanging quota or sync off \
         one still sees it; the subscription is what drops the repeat"
    );

    futures::executor::block_on(host.clear(
        &CallContext::default(),
        HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest {
            key: "progress".to_string(),
        }),
    ))
    .expect("storage clear");
    assert_eq!(next(&mut subscription), storage_item(None));
}

#[test]
fn local_storage_subscribe_is_scoped_to_the_calling_product() {
    let platform = stub_platform();
    let mine = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let other = ProductRuntimeHost::new(platform, runtime_config("other.dot"), test_spawner());
    write_storage_key(&mine, "shared", b"mine");

    let mut mine_items = subscribe_storage_key(&mine, "shared");
    let mut other_items = subscribe_storage_key(&other, "shared");
    assert_eq!(
        futures::executor::block_on(mine_items.next()),
        Some(storage_item(Some(b"mine")))
    );
    assert_eq!(
        futures::executor::block_on(other_items.next()),
        Some(storage_item(None)),
        "the same key name in another product is a different key"
    );

    write_storage_key(&mine, "shared", b"again");
    assert_eq!(
        futures::executor::block_on(mine_items.next()),
        Some(storage_item(Some(b"again")))
    );
    assert!(
        futures::FutureExt::now_or_never(other_items.next()).is_none(),
        "another product's write never reaches this subscriber"
    );
}

#[test]
fn local_storage_subscribe_interrupts_on_a_platform_stream_failure() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let mut subscription = subscribe_storage_key(&host, "progress");
    assert_eq!(
        futures::executor::block_on(subscription.next()),
        Some(storage_item(None))
    );

    platform.fail_storage_subscriptions(
        &host.product_storage_key("myapp.dot", "progress".to_string()),
        "store unavailable",
    );

    assert_eq!(
        futures::executor::block_on(subscription.next()),
        Some(Err(CallError::HostFailure {
            reason: "store unavailable".to_string(),
        })),
        "a platform failure reaches the product instead of freezing it on the last value"
    );
}

#[test]
fn worker_operations_reach_the_platform_scoped_to_the_calling_product() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();
    let begin = |label: Option<&str>| {
        let HostWorkerBeginOperationResponse::V1(response) =
            futures::executor::block_on(host.begin_operation(
                &cx,
                HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest {
                    label: label.map(str::to_string),
                }),
            ))
            .expect("begin operation");
        response.id
    };
    let end = |id: u32| {
        futures::executor::block_on(host.end_operation(
            &cx,
            HostWorkerEndOperationRequest::V1(v01::HostWorkerEndOperationRequest { id }),
        ))
        .expect("end operation")
    };

    assert_eq!(begin(Some("funding")), 1);
    assert_eq!(begin(None), 2);
    assert_eq!(
        *platform
            .begun_operations
            .lock()
            .expect("begun operations mutex poisoned"),
        vec![
            ("myapp.dot".to_string(), "funding".to_string()),
            ("myapp.dot".to_string(), String::new()),
        ],
        "begin reaches the platform under the calling product; a missing label is empty"
    );

    end(1);
    end(99);
    assert_eq!(
        *platform
            .ended_operations
            .lock()
            .expect("ended operations mutex poisoned"),
        vec![("myapp.dot".to_string(), 1), ("myapp.dot".to_string(), 99)],
        "end reaches the platform under the calling product, unknown ids included"
    );
}

#[test]
fn an_open_operation_holds_worker_demand_until_it_ends() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();
    let ledger = &host.services().worker_ledger;

    let HostWorkerBeginOperationResponse::V1(response) =
        futures::executor::block_on(host.begin_operation(
            &cx,
            HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest {
                label: Some("funding".to_string()),
            }),
        ))
        .expect("begin operation");

    assert_eq!(
        ledger.count("myapp.dot"),
        1,
        "an open operation is demand on the worker, so the host is told to run it"
    );

    futures::executor::block_on(host.end_operation(
        &cx,
        HostWorkerEndOperationRequest::V1(v01::HostWorkerEndOperationRequest { id: response.id }),
    ))
    .expect("end operation");

    assert_eq!(
        ledger.count("myapp.dot"),
        0,
        "ending the last operation drops the demand it held"
    );
}

#[test]
fn ending_an_operation_twice_releases_only_the_demand_it_held() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();
    let ledger = &host.services().worker_ledger;
    let begin = || {
        let HostWorkerBeginOperationResponse::V1(response) =
            futures::executor::block_on(host.begin_operation(
                &cx,
                HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest {
                    label: None,
                }),
            ))
            .expect("begin operation");
        response.id
    };
    let end = |id: u32| {
        futures::executor::block_on(host.end_operation(
            &cx,
            HostWorkerEndOperationRequest::V1(v01::HostWorkerEndOperationRequest { id }),
        ))
        .expect("end operation");
    };

    let first = begin();
    begin();
    assert_eq!(ledger.count("myapp.dot"), 2);

    end(first);
    end(first);

    assert_eq!(
        ledger.count("myapp.dot"),
        1,
        "a repeated end is idempotent, so it cannot drop the demand the other operation holds"
    );
}

/// Records every worker-demand transition the ledger reports.
#[derive(Default)]
struct DemandRecorder {
    transitions: Mutex<Vec<(String, crate::host_logic::worker::WorkerTransition)>>,
}

impl DemandRecorder {
    fn seen(&self) -> Vec<(String, crate::host_logic::worker::WorkerTransition)> {
        self.transitions
            .lock()
            .expect("demand recorder mutex poisoned")
            .clone()
    }
}

impl crate::host_logic::worker::WorkerDemandObserver for DemandRecorder {
    fn worker_demand_changed(
        &self,
        product_id: &str,
        transition: crate::host_logic::worker::WorkerTransition,
    ) {
        self.transitions
            .lock()
            .expect("demand recorder mutex poisoned")
            .push((product_id.to_string(), transition));
    }
}

#[test]
fn tearing_down_a_connection_reports_the_stop_before_its_last_reference_goes() {
    use crate::host_logic::worker::WorkerTransition;

    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let recorder = Arc::new(DemandRecorder::default());
    assert!(
        host.services()
            .worker_ledger
            .install_demand_observer(recorder.clone())
    );
    let cx = CallContext::default();

    futures::executor::block_on(host.begin_operation(
        &cx,
        HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest { label: None }),
    ))
    .expect("begin operation");

    host.release_open_operations();

    // A native reconnect drops the replaced connection's last reference only
    // after the new one exists, so a stop deferred to that point would reach
    // the host as a stop for the worker it had just restarted.
    assert_eq!(
        recorder.seen(),
        vec![
            ("myapp.dot".to_string(), WorkerTransition::Start),
            ("myapp.dot".to_string(), WorkerTransition::Stop),
        ],
        "teardown reports the stop, rather than leaving it to the last Arc"
    );

    drop(host);

    assert_eq!(
        recorder.seen().len(),
        2,
        "the eventual drop has nothing left to report"
    );
}

#[test]
fn a_cancelled_begin_ends_the_operation_the_host_started() {
    let (release, gate) = futures::channel::oneshot::channel();
    let platform = Arc::new(StubPlatform {
        begin_operation_gate: Mutex::new(Some(gate)),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();

    // Poll once so the call reaches the host, then drop it the way an aborted
    // dispatch does while the host is still deciding.
    let mut begun = Box::pin(host.begin_operation(
        &cx,
        HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest { label: None }),
    ));
    assert!(
        futures::executor::block_on(futures::future::poll_fn(|cx| {
            std::task::Poll::Ready(futures::FutureExt::poll_unpin(&mut begun, cx).is_pending())
        })),
        "the host has not answered yet"
    );
    drop(begun);

    // Without the fix the host's call went with the cancelled dispatch, so the
    // gate may already be gone.
    let _ = release.send(());

    // Nobody is left to receive the id, so the operation the host started is
    // ended rather than stranded in its store.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let ended = platform
            .ended_operations
            .lock()
            .expect("ended operations mutex poisoned")
            .clone();
        if ended == vec![("myapp.dot".to_string(), 1)] {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "a cancelled begin leaves the host holding nothing; saw {ended:?}"
        );
        std::thread::yield_now();
    }
    assert_eq!(host.services().worker_ledger.count("myapp.dot"), 0);
}

#[test]
fn a_failed_end_still_drops_the_demand_the_operation_held() {
    let platform = Arc::new(StubPlatform {
        end_operation_error: Some("store unavailable"),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let cx = CallContext::default();
    let ledger = &host.services().worker_ledger;

    let HostWorkerBeginOperationResponse::V1(response) =
        futures::executor::block_on(host.begin_operation(
            &cx,
            HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest {
                label: None,
            }),
        ))
        .expect("begin operation");
    assert_eq!(ledger.count("myapp.dot"), 1);

    let ended = futures::executor::block_on(host.end_operation(
        &cx,
        HostWorkerEndOperationRequest::V1(v01::HostWorkerEndOperationRequest { id: response.id }),
    ));
    assert!(
        ended.is_err(),
        "the host's failure still reaches the product"
    );

    assert_eq!(
        ledger.count("myapp.dot"),
        0,
        "the product declared the operation over, so the core stops counting it \
         whatever the host made of the call"
    );
}

#[test]
fn dropping_a_connection_releases_the_demand_its_open_operations_held() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let services = host.services().clone();
    let cx = CallContext::default();

    futures::executor::block_on(host.begin_operation(
        &cx,
        HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest { label: None }),
    ))
    .expect("begin operation");
    assert_eq!(services.worker_ledger.count("myapp.dot"), 1);

    drop(host);

    assert_eq!(
        services.worker_ledger.count("myapp.dot"),
        0,
        "a product that goes away without ending its operations leaves no demand behind"
    );
}

#[test]
fn theme_subscribe_maps_platform_values() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let mut subscription = futures::executor::block_on(Theme::subscribe(
        &host,
        &cx,
        truapi::versioned::theme::HostThemeSubscribeRequest::V1,
    ));
    let item = futures::executor::block_on(subscription.next()).expect("theme item");
    assert_eq!(
        item,
        Ok(HostThemeSubscribeItem::V1(v01::HostThemeSubscribeItem {
            name: v01::ThemeName::Custom("midnight".to_string()),
            variant: v01::ThemeVariant::Dark,
        }))
    );
}

#[test]
fn idle_peer_disconnect_monitor_clears_session_store_and_broadcasts() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform {
        rpc_responses: sso_peer_disconnect_monitor_responses(&session),
        ..Default::default()
    });
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let mut statuses = host.test_session_state().subscribe();
    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Connected
        )
    );

    pairing_host.start_session_supervision_for_current_session();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let disconnected = loop {
        if let Some(item) = statuses.next().now_or_never() {
            break item.expect("status stream ended");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "peer disconnect monitor did not emit Disconnected"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    };

    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *platform
            .session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        1
    );
    assert_eq!(
        disconnected,
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Disconnected
        )
    );
}

#[test]
fn legacy_create_transaction_rejects_raw_key_mismatch() {
    let host =
        ProductRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
    install_pairing_session(&host, sso_session_info());
    let cx = CallContext::default();
    let request = HostCreateTransactionWithLegacyAccountRequest::V1(v01::LegacyAccountTxPayload {
        signer: [0; 32],
        genesis_hash: [1; 32],
        call_data: vec![0],
        extensions: vec![],
        tx_ext_version: 0,
    });
    let err =
        futures::executor::block_on(host.create_transaction_with_legacy_account(&cx, request))
            .unwrap_err();
    match err {
        CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(
            v01::HostCreateTransactionError::Unknown { reason },
        )) => assert_eq!(reason, "Account can't be derived from product account id"),
        other => panic!("expected legacy signer mismatch, got {other:?}"),
    }
}

#[test]
fn legacy_create_transaction_accepts_identity_account_then_routes_legacy_request() {
    let session = sso_session_info();
    let identity = session.identity_account_id.unwrap();
    let platform = Arc::new(StubPlatform {
        create_transaction_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            crate::host_logic::sso::messages::RemoteMessage {
                message_id: "wallet-identity-create-tx-1".to_string(),
                data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                    crate::host_logic::sso::messages::v1::RemoteMessage::CreateTransactionResponse(
                        crate::host_logic::sso::messages::Response {
                            responding_to: "identity-create-tx-1".to_string(),
                            payload: Ok(vec![0xca, 0xfe]),
                        },
                    ),
                ),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("identity-create-tx-1".to_string());
    let request = HostCreateTransactionWithLegacyAccountRequest::V1(v01::LegacyAccountTxPayload {
        signer: identity,
        genesis_hash: [1; 32],
        call_data: vec![0],
        extensions: vec![],
        tx_ext_version: 0,
    });

    let response =
        futures::executor::block_on(host.create_transaction_with_legacy_account(&cx, request))
            .unwrap();

    let HostCreateTransactionWithLegacyAccountResponse::V1(inner) = response;
    assert_eq!(inner.transaction, vec![0xca, 0xfe]);
    let message = submitted_remote_message(&platform, &session);
    let crate::host_logic::sso::messages::RemoteMessageData::V1(
        crate::host_logic::sso::messages::v1::RemoteMessage::CreateTransactionWithLegacyAccountRequest(
            request,
        ),
    ) = message.data
    else {
        panic!("expected identity transaction request");
    };
    let crate::host_logic::sso::messages::CreateTransactionLegacyPayload::V1(payload) =
        request.payload;
    assert_eq!(payload.signer, identity);
}

#[test]
fn legacy_create_transaction_accepts_derived_key_then_returns_sso_response() {
    let session = sso_session_info();
    let signer = test_product_account_public("myapp.dot", 0);
    let platform = Arc::new(StubPlatform {
        create_transaction_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            crate::host_logic::sso::messages::RemoteMessage {
                message_id: "wallet-legacy-create-tx-1".to_string(),
                data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                    crate::host_logic::sso::messages::v1::RemoteMessage::CreateTransactionResponse(
                        crate::host_logic::sso::messages::Response {
                            responding_to: "legacy-create-tx-1".to_string(),
                            payload: Ok(vec![0xca, 0xfe]),
                        },
                    ),
                ),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("legacy-create-tx-1".to_string());
    let request = HostCreateTransactionWithLegacyAccountRequest::V1(v01::LegacyAccountTxPayload {
        signer,
        genesis_hash: [1; 32],
        call_data: vec![0],
        extensions: vec![],
        tx_ext_version: 0,
    });

    let response =
        futures::executor::block_on(host.create_transaction_with_legacy_account(&cx, request))
            .unwrap();

    let HostCreateTransactionWithLegacyAccountResponse::V1(inner) = response;
    assert_eq!(inner.transaction, vec![0xca, 0xfe]);
    let message = submitted_remote_message(&platform, &session);
    let crate::host_logic::sso::messages::RemoteMessageData::V1(
        crate::host_logic::sso::messages::v1::RemoteMessage::CreateTransactionRequest(request),
    ) = message.data
    else {
        panic!("expected product transaction request");
    };
    let crate::host_logic::sso::messages::CreateTransactionPayload::V1(payload) = request.payload;
    assert_eq!(
        payload.signer,
        v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: v01::DerivationIndex::Index(0),
        }
    );
}

#[test]
fn resource_allocation_rejects_without_session() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let err = futures::executor::block_on(ResourceAllocation::request(
        &host,
        &cx,
        resource_allocation_request(),
    ))
    .unwrap_err();
    match err {
        CallError::Domain(HostRequestResourceAllocationError::V1(
            v01::ResourceAllocationError::Unknown { reason },
        )) => assert_eq!(reason, "No active session"),
        other => panic!("expected no-session resource allocation error, got {other:?}"),
    }
}

#[test]
fn resource_allocation_remembers_denial_without_provisioning() {
    let platform = stub_platform();
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session_info());
    futures::executor::block_on(async {
        for _ in 0..2 {
            assert!(
                ResourceAllocation::request(
                    &host,
                    &CallContext::default(),
                    resource_allocation_request(),
                )
                .await
                .is_err()
            );
        }
        assert_eq!(
            host.permission_authorization_status(
                truapi_platform::PermissionAuthorizationRequest::StatementStoreAllowance {
                    derivation_index: None,
                },
            )
            .await
            .unwrap(),
            truapi_platform::PermissionAuthorizationStatus::Denied
        );
    });
    assert_eq!(
        platform.resource_allocation_reviews.lock().unwrap().len(),
        1
    );
    assert!(platform.sent_rpc.lock().unwrap().is_empty());
}

#[test]
fn resource_allocation_maps_confirmation_failure_to_host_failure() {
    let host = ProductRuntimeHost::new_compat(
        Arc::new(StubPlatform {
            resource_allocation_error: Some("modal failed"),
            ..Default::default()
        }),
        test_spawner(),
    );
    install_pairing_session(&host, session_info());
    let cx = CallContext::default();
    let err = futures::executor::block_on(ResourceAllocation::request(
        &host,
        &cx,
        resource_allocation_request(),
    ))
    .unwrap_err();
    assert!(matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed")));
}

#[test]
fn resource_allocation_respects_a_shorter_call_context_timeout() {
    let session = sso_session_info();
    let message_id = "allocation-timeout";
    let mut rpc_responses = sso_success_responses(
        &session,
        message_id,
        crate::host_logic::sso::messages::RemoteMessage {
            message_id: "wallet-allocation-timeout".to_string(),
            data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::v1::RemoteMessage::ResourceAllocationResponse(
                    crate::host_logic::sso::messages::Response {
                        responding_to: message_id.to_string(),
                        payload: Ok(vec![]),
                    },
                ),
            ),
        },
    );
    rpc_responses.truncate(3);
    let platform = Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        rpc_responses,
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session);
    let mut cx = CallContext::with_request_id(message_id.to_string());
    cx.set_timeout(std::time::Duration::from_millis(1));

    let err = futures::executor::block_on(ResourceAllocation::request(
        &host,
        &cx,
        resource_allocation_request(),
    ))
    .unwrap_err();

    match err {
        CallError::Domain(HostRequestResourceAllocationError::V1(
            v01::ResourceAllocationError::Unknown { reason },
        )) => assert_eq!(
            reason,
            "Account authority request timed out after 1ms for allocation-timeout"
        ),
        other => panic!("expected resource-allocation timeout, got {other:?}"),
    }

    wait_until(
        || recorded_rpc_method_count(&platform.sent_rpc, "statement_unsubscribeStatement") == 2,
        "timed-out resource allocation did not unsubscribe statement streams",
    );
}

#[test]
fn resource_allocation_accepts_confirmation_then_returns_sso_response() {
    let session = sso_session_info();
    let slot_account_key = {
        let mini_secret = schnorrkel::MiniSecretKey::from_bytes(&[12; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
        keypair.secret.to_bytes().to_vec()
    };
    let platform = Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            crate::host_logic::sso::messages::RemoteMessage {
                message_id: "wallet-alloc-1".to_string(),
                data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                    crate::host_logic::sso::messages::v1::RemoteMessage::ResourceAllocationResponse(
                        crate::host_logic::sso::messages::Response {
                            responding_to: "alloc-1".to_string(),
                            payload: Ok(vec![
                                crate::host_logic::sso::messages::SsoAllocationOutcome::Allocated(
                                    crate::host_logic::sso::messages::SsoAllocatedResource::StatementStoreAllowance {
                                        slot_account_key,
                                    },
                                ),
                                crate::host_logic::sso::messages::SsoAllocationOutcome::Rejected,
                                crate::host_logic::sso::messages::SsoAllocationOutcome::NotAvailable,
                            ]),
                        },
                    ),
                ),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, session.clone());
    let cx = CallContext::with_request_id("alloc-1".to_string());
    let response = futures::executor::block_on(ResourceAllocation::request(
        &host,
        &cx,
        resource_allocation_request(),
    ))
    .unwrap();
    let HostRequestResourceAllocationResponse::V1(inner) = response;
    assert_eq!(
        inner.outcomes,
        vec![
            v01::AllocationOutcome::Allocated,
            v01::AllocationOutcome::Rejected,
            v01::AllocationOutcome::NotAvailable,
        ]
    );
    let message = submitted_remote_message(&platform, &session);
    assert!(matches!(
        message.data,
        crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::v1::RemoteMessage::ResourceAllocationRequest(_)
        )
    ));
}

fn auto_signing_test_platform(session: &SessionInfo, request_id: &str) -> Arc<StubPlatform> {
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            session,
            RemoteMessage {
                message_id: format!("wallet-{request_id}"),
                data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(
                    crate::host_logic::sso::messages::Response {
                        responding_to: request_id.to_string(),
                        payload: Ok(vec![
                            crate::host_logic::sso::messages::SsoAllocationOutcome::Allocated(
                                crate::host_logic::sso::messages::SsoAllocatedResource::AutoSigning {
                                    product_root_private_key: subtree.secret.to_bytes(),
                                    ring_vrf_domain_entropy:
                                        crate::host_logic::product_account::derive_ring_vrf_domain_entropy(
                                            &[0xAB; 16],
                                            "myapp.dot",
                                        )
                                        .unwrap(),
                                },
                            ),
                        ]),
                    },
                )),
            },
        )),
        ..Default::default()
    })
}

fn request_auto_signing(host: &ProductRuntimeHost, request_id: &str) {
    futures::executor::block_on(ResourceAllocation::request(
        host,
        &CallContext::with_request_id(request_id.to_string()),
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources: vec![v01::AllocatableResource::AutoSigning],
        }),
    ))
    .expect("AutoSigning allocation succeeds");
}

fn auto_signing_vrf_request() -> HostAccountSignVrfRequest {
    HostAccountSignVrfRequest::V1(v01::HostAccountSignVrfRequest {
        account: account_id("myapp.dot", 0),
        transcript_label: b"ctx".to_vec(),
        items: vec![v01::VrfTranscriptItem {
            label: b"round".to_vec(),
            value: vec![7],
        }],
    })
}

/// A pairing host holding an AutoSigning capability, with its SSO script
/// already spent on the allocation: anything that relays from here fails, so a
/// call that succeeds was served locally.
fn granted_pairing_host() -> (Arc<StubPlatform>, ProductRuntimeHost) {
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-1");
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);
    request_auto_signing(&host, "auto-1");
    (platform, host)
}

/// The product account the capability in [`granted_pairing_host`] derives.
fn granted_keypair() -> schnorrkel::Keypair {
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    crate::host_logic::product_account::derive_product_keypair(&root, "myapp.dot", index_bytes(0))
        .unwrap()
}

#[test]
fn auto_signing_serves_sign_raw_locally_without_prompt_or_sso() {
    let (platform, host) = granted_pairing_host();

    let HostSignRawResponse::V1(response) = futures::executor::block_on(host.sign_raw(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        }),
    ))
    .expect("the capability signs locally, with no signing host to relay to");

    assert!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .is_empty(),
        "the grant waives the prompt",
    );
    let signature =
        schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
    let keypair = granted_keypair();
    assert!(
        keypair
            .public
            .verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
            .is_ok(),
        "the local signature is over the watermarked bytes, by the product account",
    );
}

#[test]
fn auto_signing_serves_create_transaction_v4_locally_without_prompt() {
    let (platform, host) = granted_pairing_host();

    let HostCreateTransactionResponse::V1(response) =
        futures::executor::block_on(host.create_transaction(
            &CallContext::default(),
            HostCreateTransactionRequest::V1(v01::ProductAccountTxPayload {
                signer: account_id("myapp.dot", 0),
                genesis_hash: [1; 32],
                call_data: vec![0x04, 0x00],
                extensions: vec![],
                // V4 needs no chain metadata, so the whole assembly is local.
                tx_ext_version: 0,
            }),
        ))
        .expect("a V4 transaction assembles locally under the capability");

    assert!(
        platform
            .create_transaction_reviews
            .lock()
            .expect("create transaction review list mutex poisoned")
            .is_empty(),
        "the grant waives the prompt",
    );
    let (signer, _, call) = crate::host_logic::extrinsic::tests::split_v4(&response.transaction);
    assert_eq!(
        signer,
        granted_keypair().public.to_bytes(),
        "the product account signed it",
    );
    assert_eq!(call, vec![0x04, 0x00]);
}

#[test]
fn auto_signing_serves_sign_payload_locally_without_prompt() {
    let (platform, host) = granted_pairing_host();
    let payload = crate::test_support::sign_payload_data();
    let preimage = crate::host_logic::transaction::extrinsic_payload_preimage(&payload)
        .expect("preimage builds");

    let HostSignPayloadResponse::V1(response) = futures::executor::block_on(host.sign_payload(
        &CallContext::default(),
        HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload,
        }),
    ))
    .expect("the capability signs the payload locally");

    assert!(
        platform
            .sign_payload_reviews
            .lock()
            .expect("sign payload review list mutex poisoned")
            .is_empty(),
        "the grant waives the prompt",
    );
    // A `MultiSignature`: the sr25519 discriminant, then the raw signature.
    assert_eq!(response.signature.len(), 65);
    assert_eq!(response.signature[0], 1);
    let signature =
        schnorrkel::Signature::from_bytes(&response.signature[1..]).expect("64-byte signature");
    assert!(
        granted_keypair()
            .public
            .verify_simple(b"substrate", &preimage, &signature)
            .is_ok(),
        "the local signature is over the payload preimage, by the product account",
    );
}

#[test]
fn auto_signing_serves_a_product_statement_proof_on_a_pairing_host() {
    // The SSO raw-signing protocol cannot carry an exact, unwatermarked
    // payload, so without a capability this role answers `UnableToSign`. The
    // capability's own key is what makes the operation available at all;
    // neither role prompts for statement proofs either way.
    let (_platform, host) = granted_pairing_host();

    let response = futures::executor::block_on(StatementStore::create_proof(
        &host,
        &CallContext::default(),
        RemoteStatementStoreCreateProofRequest::V1(
            truapi::latest::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("myapp.dot", 0),
                statement: statement(),
            },
        ),
    ))
    .expect("the capability signs the statement locally");

    let RemoteStatementStoreCreateProofResponse::V1(inner) = response;
    let truapi::latest::StatementProof::Sr25519 { signer, signature } = inner.proof else {
        panic!("expected an sr25519 statement proof");
    };
    let keypair = granted_keypair();
    assert_eq!(
        signer,
        keypair.public.to_bytes(),
        "the product account signed it",
    );
    let payload = crate::host_logic::statement_store::unsigned_statement_signing_payload(
        crate::host_logic::statement_store::statement_fields_from_v01(statement()).unwrap(),
    )
    .unwrap();
    let signature = schnorrkel::Signature::from_bytes(&signature).expect("64-byte signature");
    assert!(
        keypair
            .public
            .verify_simple(b"substrate", &payload, &signature)
            .is_ok(),
        "the signature is over the statement's signing payload",
    );
}

#[test]
fn a_broken_auto_signing_slot_fails_sign_raw_rather_than_prompting() {
    // The lookup erases a capability it cannot trust as it rejects it, so
    // falling through to a prompt here would ask the user to approve a
    // signature the host has already refused to make. This is what the grant
    // query's `Result` return is for: a `bool` predicate would answer "no
    // grant" and raise the modal.
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-broken");
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    request_auto_signing(&host, "auto-broken");

    let expected_subtree = test_product_subtree("myapp.dot");
    let storage_key = core_storage_test_key(CoreStorageKey::AutoSigningKeys);
    {
        let mut storage = platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned");
        let blob = storage
            .get_mut(&storage_key)
            .expect("scoped AutoSigning capability persisted");
        let expected_offset = blob
            .windows(expected_subtree.len())
            .position(|window| window == expected_subtree)
            .expect("persisted expected subtree is present");
        blob[expected_offset] ^= 0x01;
    }

    let restored = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&restored, session);
    let error = futures::executor::block_on(restored.sign_raw(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        }),
    ))
    .unwrap_err();

    assert!(
        matches!(
            &error,
            CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown {
                reason
            })) if reason == "AutoSigning capability is not for the current product subtree"
        ),
        "the broken capability is reported, not silently downgraded: {error:?}",
    );
    assert!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .is_empty(),
        "no prompt is raised for a capability the host just erased",
    );
}

#[test]
fn an_unwatermarked_sign_raw_still_prompts_under_a_grant() {
    let (platform, host) = granted_pairing_host();

    #[allow(deprecated)]
    let error = futures::executor::block_on(host.sign_raw_unwatermarked_deprecated(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: v01::RawPayload::Bytes {
                bytes: b"hello world".to_vec(),
            },
        }),
    ))
    .expect_err("the stub declines the confirmation");

    assert!(matches!(
        error,
        CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
    ));
    assert_eq!(
        platform
            .sign_raw_reviews
            .lock()
            .expect("raw signing review list mutex poisoned")
            .len(),
        1,
        "the deprecated API prompts whatever the capability says",
    );
}

#[test]
fn auto_signing_allocation_persists_and_serves_vrf_without_sso() {
    let session = sso_session_info();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    let product_root_private_key = subtree.secret.to_bytes();
    let platform = Arc::new(StubPlatform {
        resource_allocation_confirmed: true,
        sso_response_script: Some(sso_success_response_script(
            &session,
            RemoteMessage {
                message_id: "wallet-auto-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(
                    crate::host_logic::sso::messages::Response {
                        responding_to: "auto-1".to_string(),
                        payload: Ok(vec![
                            crate::host_logic::sso::messages::SsoAllocationOutcome::Allocated(
                                crate::host_logic::sso::messages::SsoAllocatedResource::AutoSigning {
                                    product_root_private_key,
                                    ring_vrf_domain_entropy:
                                        crate::host_logic::product_account::derive_ring_vrf_domain_entropy(
                                            &[0xAB; 16],
                                            "myapp.dot",
                                        )
                                        .unwrap(),
                                },
                            ),
                        ]),
                    },
                )),
            },
        )),
        ..Default::default()
    });
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let allocation =
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources: vec![v01::AllocatableResource::AutoSigning],
        });
    futures::executor::block_on(ResourceAllocation::request(
        &host,
        &CallContext::with_request_id("auto-1".to_string()),
        allocation,
    ))
    .expect("AutoSigning allocation succeeds");

    let restored = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    restored.test_session_state().set_session(session);
    let request = v01::HostAccountSignVrfRequest {
        account: account_id("myapp.dot", 0),
        transcript_label: b"ctx".to_vec(),
        items: vec![v01::VrfTranscriptItem {
            label: b"round".to_vec(),
            value: vec![7],
        }],
    };
    let response = futures::executor::block_on(restored.sign_vrf(
        &CallContext::default(),
        HostAccountSignVrfRequest::V1(request),
    ))
    .expect("persisted AutoSigning key signs locally");
    let HostAccountSignVrfResponse::V1(signature) = response;
    assert!(
        platform
            .sign_vrf_reviews
            .lock()
            .expect("VRF signing review list mutex poisoned")
            .is_empty()
    );

    let keypair = crate::host_logic::product_account::derive_product_keypair(
        &root,
        "myapp.dot",
        index_bytes(0),
    )
    .unwrap();
    let mut transcript = merlin::Transcript::new(b"ctx");
    transcript.append_message(b"round", &[7]);
    let pre_output = schnorrkel::vrf::VRFPreOut::from_bytes(&signature.pre_output).unwrap();
    let proof = schnorrkel::vrf::VRFProof::from_bytes(&signature.proof).unwrap();
    keypair
        .public
        .vrf_verify(transcript, &pre_output, &proof)
        .expect("local AutoSigning VRF verifies");
}

#[test]
fn ring_vrf_sign_reports_not_connected_before_foreign_key_policy() {
    let host = ProductRuntimeHost::new(
        Arc::new(StubPlatform::default()),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    let request = HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
        key_handle: account_id("other.dot", 0),
        message: b"not connected".to_vec(),
    });

    let error = futures::executor::block_on(host.ring_vrf_sign(&CallContext::default(), request))
        .unwrap_err();

    assert!(matches!(
        error,
        CallError::Domain(HostAccountRingVrfSignError::V1(
            v01::HostAccountRingVrfSignError::NotConnected
        ))
    ));
}

#[test]
fn auto_signing_ring_vrf_requires_registration_and_signs_locally() {
    use verifiable::GenerateVerifiable;
    use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

    let session = sso_session_info();
    let platform = Arc::new(StubPlatform::default());
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        ProductRuntimeHost::compat_host_config(),
        ProductContext::new("myapp.dot".to_string()).unwrap(),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());

    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    let domain = crate::host_logic::product_account::derive_ring_vrf_domain_entropy(
        &[0xAB; 16],
        "myapp.dot",
    )
    .unwrap();
    futures::executor::block_on(pairing_host.remember_auto_signing_key_for_tests(
        &session,
        pairing_host.current_session_lifecycle_epoch(),
        "myapp.dot",
        subtree.public.to_bytes(),
        subtree.secret.to_bytes(),
        domain,
    ))
    .unwrap();

    let handle = account_id("myapp.dot", 7);
    let request = HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
        key_handle: handle.clone(),
        message: b"registered keys only".to_vec(),
    });
    let error = futures::executor::block_on(host.ring_vrf_sign(&CallContext::default(), request))
        .unwrap_err();
    assert!(matches!(
        error,
        CallError::Domain(HostAccountRingVrfSignError::V1(
            v01::HostAccountRingVrfSignError::KeyNotRegistered
        ))
    ));

    let ring = ring_location_fixture();
    let entropy = crate::host_logic::product_account::derive_ring_vrf_entropy_from_domain(
        &domain,
        &handle.derivation_index,
    );
    let public_key = crate::runtime::signing_host::ring_vrf::member_from_entropy(&entropy).unwrap();
    futures::executor::block_on(pairing_host.register_ring_vrf_key_for_tests(
        &session,
        handle.clone(),
        ring,
        public_key,
    ))
    .unwrap();

    let message = b"registered keys only".to_vec();
    let response = futures::executor::block_on(host.ring_vrf_sign(
        &CallContext::default(),
        HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
            key_handle: handle,
            message: message.clone(),
        }),
    ))
    .unwrap();
    let HostAccountRingVrfSignResponse::V1(signature) = response;
    let signature: [u8; 64] = signature
        .try_into()
        .expect("fixed-width ring-VRF signature");
    assert!(BandersnatchVrfVerifiable::verify_signature(
        &signature,
        &message,
        &public_key
    ));
    assert!(
        platform
            .sent_rpc
            .lock()
            .expect("sent RPC mutex poisoned")
            .is_empty(),
        "registered AutoSigning ring-VRF use stays local"
    );

    let mismatched_handle = account_id("myapp.dot", 8);
    futures::executor::block_on(pairing_host.register_ring_vrf_key_for_tests(
        &session,
        mismatched_handle.clone(),
        ring_location_fixture(),
        [0xFF; 32],
    ))
    .unwrap();
    let error = futures::executor::block_on(host.ring_vrf_sign(
        &CallContext::default(),
        HostAccountRingVrfSignRequest::V1(v01::HostAccountRingVrfSignRequest {
            key_handle: mismatched_handle,
            message: b"reject mismatched registry state".to_vec(),
        }),
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        CallError::Domain(HostAccountRingVrfSignError::V1(
            v01::HostAccountRingVrfSignError::Unknown { reason }
        )) if reason.contains("does not match the AutoSigning capability")
    ));
}

#[test]
fn auto_signing_rejects_persisted_key_for_unexpected_product_subtree() {
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-tamper");
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    request_auto_signing(&host, "auto-tamper");

    let expected_subtree = test_product_subtree("myapp.dot");
    let storage_key = core_storage_test_key(CoreStorageKey::AutoSigningKeys);
    {
        let mut storage = platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned");
        let blob = storage
            .get_mut(&storage_key)
            .expect("scoped AutoSigning capability persisted");
        let expected_offset = blob
            .windows(expected_subtree.len())
            .position(|window| window == expected_subtree)
            .expect("persisted expected subtree is present");
        blob[expected_offset] ^= 0x01;
    }

    let restored = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&restored, session);
    let err = futures::executor::block_on(
        restored.sign_vrf(&CallContext::default(), auto_signing_vrf_request()),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountSignVrfError::V1(
            v01::HostAccountSignVrfError::Unknown { reason }
        )) if reason == "AutoSigning capability is not for the current product subtree"
    ));
    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&storage_key)
    );
}

#[test]
fn auto_signing_logout_reset_clears_cached_and_persisted_capability() {
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-logout");
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    request_auto_signing(&host, "auto-logout");
    assert!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
    );

    futures::executor::block_on(pairing_host.logout_and_reset_pairing()).unwrap();

    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
    );
    assert!(
        !futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&session, "myapp.dot")
        )
        .expect("AutoSigning storage remains readable"),
        "logout must evict the in-memory AutoSigning capability"
    );
}

#[test]
fn stale_secret_allocations_cannot_persist_after_reset_and_same_owner_reactivation() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform::default());
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let stale_epoch = pairing_host.current_session_lifecycle_epoch();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();

    futures::executor::block_on(pairing_host.logout_and_reset_pairing()).unwrap();
    futures::executor::block_on(pairing_host.set_connected_session_for_tests(session.clone()));
    let auto_signing_error =
        futures::executor::block_on(pairing_host.remember_auto_signing_key_for_tests(
            &session,
            stale_epoch,
            "myapp.dot",
            subtree.public.to_bytes(),
            subtree.secret.to_bytes(),
            [0x42; 32],
        ))
        .expect_err("the old AutoSigning allocation completion must be rejected");
    let statement_store_result =
        futures::executor::block_on(pairing_host.cache_statement_store_allowance_key(
            &session,
            stale_epoch,
            "myapp.dot",
            subtree.secret.to_bytes().to_vec(),
        ));
    let Err(statement_store_error) = statement_store_result else {
        panic!("the old statement-store allocation completion must be rejected");
    };
    let bulletin_result = futures::executor::block_on(pairing_host.cache_bulletin_allowance_key(
        &session,
        stale_epoch,
        "myapp.dot",
        subtree.secret.to_bytes().to_vec(),
    ));
    let Err(bulletin_error) = bulletin_result else {
        panic!("the old Bulletin allocation completion must be rejected");
    };

    assert!(matches!(auto_signing_error, AuthorityError::Disconnected));
    assert!(matches!(
        statement_store_error,
        AuthorityError::Disconnected
    ));
    assert!(matches!(bulletin_error, AuthorityError::Disconnected));
    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys)),
        "the stale allocation must not restore durable AutoSigning authority"
    );
    assert!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .is_empty(),
        "stale allowance completions must not restore durable slot secrets"
    );
    assert!(
        !futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&session, "myapp.dot")
        )
        .expect("AutoSigning storage remains readable"),
        "the stale allocation must not restore cached AutoSigning authority"
    );
}

#[test]
fn product_clear_preserves_other_capabilities_and_fences_stale_work() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform::default());
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    cache_test_product_subtree(&host, &session, "other.dot");
    let stale_epoch = pairing_host.current_session_lifecycle_epoch();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let first =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    let other =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "other.dot")
            .unwrap();

    for (product_id, subtree) in [("myapp.dot", &first), ("other.dot", &other)] {
        futures::executor::block_on(pairing_host.remember_auto_signing_key_for_tests(
            &session,
            stale_epoch,
            product_id,
            subtree.public.to_bytes(),
            subtree.secret.to_bytes(),
            [0x42; 32],
        ))
        .unwrap();
        futures::executor::block_on(pairing_host.cache_statement_store_allowance_key(
            &session,
            stale_epoch,
            product_id,
            subtree.secret.to_bytes().to_vec(),
        ))
        .unwrap();
        futures::executor::block_on(pairing_host.cache_bulletin_allowance_key(
            &session,
            stale_epoch,
            product_id,
            subtree.secret.to_bytes().to_vec(),
        ))
        .unwrap();
    }

    futures::executor::block_on(pairing_host.clear_product_state("myapp.dot")).unwrap();
    let current_epoch = pairing_host.current_session_lifecycle_epoch();
    assert_ne!(current_epoch, stale_epoch);
    assert_eq!(
        pairing_host.capability_cache_sizes_for_tests(),
        (1, 1, 1, 1)
    );
    assert!(
        !futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&session, "myapp.dot")
        )
        .unwrap()
    );
    assert!(
        futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&session, "other.dot")
        )
        .unwrap()
    );
    assert!(
        futures::executor::block_on(pairing_host.cached_statement_store_allowance_key(
            &session,
            current_epoch,
            "myapp.dot",
        ))
        .unwrap()
        .is_none()
    );
    assert!(
        futures::executor::block_on(pairing_host.cached_statement_store_allowance_key(
            &session,
            current_epoch,
            "other.dot",
        ))
        .unwrap()
        .is_some()
    );
    assert!(
        futures::executor::block_on(pairing_host.cached_bulletin_allowance_key(
            &session,
            current_epoch,
            "myapp.dot",
        ))
        .unwrap()
        .is_none()
    );
    assert!(
        futures::executor::block_on(pairing_host.cached_bulletin_allowance_key(
            &session,
            current_epoch,
            "other.dot",
        ))
        .unwrap()
        .is_some()
    );

    assert!(matches!(
        futures::executor::block_on(pairing_host.remember_auto_signing_key_for_tests(
            &session,
            stale_epoch,
            "myapp.dot",
            first.public.to_bytes(),
            first.secret.to_bytes(),
            [0x42; 32],
        )),
        Err(AuthorityError::Disconnected)
    ));
    assert!(matches!(
        futures::executor::block_on(pairing_host.cache_statement_store_allowance_key(
            &session,
            stale_epoch,
            "myapp.dot",
            first.secret.to_bytes().to_vec(),
        )),
        Err(AuthorityError::Disconnected)
    ));
    assert_eq!(
        pairing_host.capability_cache_sizes_for_tests(),
        (1, 1, 1, 1)
    );
    assert_eq!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .len(),
        2,
        "the aggregate AutoSigning and active-session allowance blobs remain for other.dot"
    );
}

#[test]
fn reset_session_state_clears_all_capabilities_without_peer_traffic() {
    let session = sso_session_info();
    let platform = Arc::new(StubPlatform::default());
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    let lifecycle_epoch = pairing_host.current_session_lifecycle_epoch();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    futures::executor::block_on(pairing_host.remember_auto_signing_key_for_tests(
        &session,
        lifecycle_epoch,
        "myapp.dot",
        subtree.public.to_bytes(),
        subtree.secret.to_bytes(),
        [0x42; 32],
    ))
    .unwrap();
    futures::executor::block_on(pairing_host.cache_statement_store_allowance_key(
        &session,
        lifecycle_epoch,
        "myapp.dot",
        subtree.secret.to_bytes().to_vec(),
    ))
    .unwrap();
    futures::executor::block_on(pairing_host.cache_bulletin_allowance_key(
        &session,
        lifecycle_epoch,
        "myapp.dot",
        subtree.secret.to_bytes().to_vec(),
    ))
    .unwrap();
    assert_eq!(
        pairing_host.capability_cache_sizes_for_tests(),
        (1, 1, 1, 1)
    );

    futures::executor::block_on(pairing_host.reset_session_state());

    assert!(pairing_host.session_state().current().is_none());
    assert_eq!(
        pairing_host.capability_cache_sizes_for_tests(),
        (0, 0, 0, 0)
    );
    assert!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .is_empty()
    );
    assert!(
        platform
            .sent_rpc
            .lock()
            .expect("sent RPC mutex poisoned")
            .is_empty(),
        "canonical reset must not submit a duplicate peer-disconnect statement"
    );
}

#[test]
fn identity_replacement_clears_all_stale_wallet_capabilities() {
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-replace");
    let (host_config, product) = runtime_config("myapp.dot");
    let (host, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&host, session.clone());
    request_auto_signing(&host, "auto-replace");
    let lifecycle_epoch = pairing_host.current_session_lifecycle_epoch();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    futures::executor::block_on(pairing_host.cache_statement_store_allowance_key(
        &session,
        lifecycle_epoch,
        "myapp.dot",
        subtree.secret.to_bytes().to_vec(),
    ))
    .unwrap();
    futures::executor::block_on(pairing_host.cache_bulletin_allowance_key(
        &session,
        lifecycle_epoch,
        "myapp.dot",
        subtree.secret.to_bytes().to_vec(),
    ))
    .unwrap();

    let mut replacement = session;
    replacement.public_key = [0x44; 32];
    replacement
        .sso
        .as_mut()
        .expect("fixture has SSO identity")
        .identity_account_id = [0x55; 32];
    futures::executor::block_on(pairing_host.set_connected_session_for_tests(replacement.clone()));

    assert_eq!(
        host.test_session_state().current(),
        Some(replacement.clone())
    );
    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
    );
    assert!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .is_empty(),
        "a replacement wallet must not leave the prior session's durable allowance secrets"
    );
    assert!(
        !futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&replacement, "myapp.dot")
        )
        .expect("AutoSigning storage remains readable"),
        "a newly paired wallet must not reuse the previous wallet's cached capability"
    );
}

#[test]
fn auto_signing_restored_different_wallet_rejects_persisted_capability() {
    let session = sso_session_info();
    let platform = auto_signing_test_platform(&session, "auto-other-wallet");
    let original = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&original, session.clone());
    request_auto_signing(&original, "auto-other-wallet");

    let mut replacement = session;
    replacement.public_key = [0x66; 32];
    replacement
        .sso
        .as_mut()
        .expect("fixture has SSO identity")
        .identity_account_id = [0x77; 32];
    let (host_config, product) = runtime_config("myapp.dot");
    let (restored, pairing_host) = ProductRuntimeHost::new_pairing_for_tests(
        platform.clone(),
        host_config,
        product,
        test_spawner(),
    );
    install_pairing_session(&restored, replacement.clone());

    assert!(
        !futures::executor::block_on(
            pairing_host.has_auto_signing_key_for_tests(&replacement, "myapp.dot")
        )
        .expect("AutoSigning storage remains readable"),
        "a different restored wallet must not use a prior wallet's capability"
    );
    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(CoreStorageKey::AutoSigningKeys))
    );
}

#[test]
fn auto_signing_rejects_and_erases_legacy_unscoped_secret() {
    let session = sso_session_info();
    let root =
        crate::host_logic::product_account::derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
    let subtree =
        crate::host_logic::product_account::derive_product_subtree_keypair(&root, "myapp.dot")
            .unwrap();
    let platform = Arc::new(StubPlatform::default());
    let legacy_key = CoreStorageKey::AutoSigningKey {
        product_id: "myapp.dot".to_string(),
    };
    platform
        .local_storage
        .lock()
        .expect("local storage mutex poisoned")
        .insert(
            core_storage_test_key(legacy_key.clone()),
            subtree.secret.to_bytes().to_vec(),
        );
    let host = ProductRuntimeHost::new(
        platform.clone(),
        runtime_config("myapp.dot"),
        test_spawner(),
    );
    install_pairing_session(&host, session);

    let err = futures::executor::block_on(
        host.sign_vrf(&CallContext::default(), auto_signing_vrf_request()),
    )
    .unwrap_err();

    assert!(matches!(
        err,
        CallError::Domain(HostAccountSignVrfError::V1(
            v01::HostAccountSignVrfError::Unknown { reason }
        )) if reason == "legacy unscoped AutoSigning capability was rejected"
    ));
    assert!(
        !platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(legacy_key))
    );
}

#[test]
fn external_session_activation_is_memory_only_and_rejects_trailing_bytes() {
    let platform = Arc::new(StubPlatform::default());
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    let session = sso_session_info();
    let blob = crate::host_logic::session::encode_persisted_session(&session);

    futures::executor::block_on(pairing_host.activate_external_session(&blob))
        .expect("valid external session activates");

    assert_eq!(host.test_session_state().current(), Some(session.clone()));
    assert!(
        platform
            .session_writes
            .lock()
            .expect("session write list mutex poisoned")
            .is_empty(),
        "external activation must not copy the blob into core storage"
    );

    let invalid = futures::executor::block_on(pairing_host.activate_external_session(&[0xff]))
        .expect_err("invalid bytes are rejected");
    assert!(invalid.starts_with("invalid session blob:"));

    let mut trailing = blob;
    trailing.push(0);
    let error = futures::executor::block_on(pairing_host.activate_external_session(&trailing))
        .expect_err("trailing bytes are rejected");
    assert_eq!(error, "invalid session blob: trailing bytes");
    assert_eq!(
        host.test_session_state().current(),
        Some(session),
        "invalid replacement preserves the active external session"
    );
}

#[test]
fn external_session_activation_reports_its_outcome_when_the_blob_is_corrupt() {
    let platform = Arc::new(StubPlatform::default());
    let (_host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    futures::executor::block_on(pairing_host.activate_external_session(&[0xff]))
        .expect_err("invalid bytes are rejected");

    // The decode fails before any transition can run, so without an
    // explicit announcement a host that holds its own session and boots on
    // a corrupt blob would hear nothing at all.
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Disconnected],
        "an activation that failed must still tell the host where it stands"
    );
}

#[test]
fn resetting_session_state_reports_its_outcome_when_nothing_was_active() {
    let platform = Arc::new(StubPlatform::default());
    let (_host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    futures::executor::block_on(pairing_host.reset_session_state());

    // Clearing an already-signed-out state changes nothing, so without an
    // explicit announcement this is the silent case a host cannot tell
    // apart from having had no answer yet.
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Disconnected],
        "a reset must still tell the host where it stands"
    );
}

#[test]
fn external_session_activation_replaces_and_fences_the_previous_session() {
    let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
        Arc::new(StubPlatform::default()),
        test_spawner(),
    );
    let first = sso_session_info();
    let mut replacement = first.clone();
    replacement.public_key = [0x44; 32];
    replacement.identity_account_id = Some([0x55; 32]);
    replacement
        .sso
        .as_mut()
        .expect("fixture has SSO")
        .identity_account_id = [0x55; 32];

    futures::executor::block_on(pairing_host.activate_external_session(
        &crate::host_logic::session::encode_persisted_session(&first),
    ))
    .expect("first external session activates");
    futures::executor::block_on(pairing_host.activate_external_session(
        &crate::host_logic::session::encode_persisted_session(&replacement),
    ))
    .expect("replacement external session activates");

    assert_eq!(host.test_session_state().current(), Some(replacement));
}

#[test]
fn store_notification_during_external_activation_restores_persisted_session() {
    let persisted = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &persisted,
        )),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || host.test_session_state().current() == Some(persisted.clone()),
        "initial persisted session was not restored",
    );

    let mut stale_external = persisted.clone();
    stale_external.public_key = [0x44; 32];
    let stale_blob = crate::host_logic::session::encode_persisted_session(&stale_external);
    let (activation_entered, resume_activation) =
        pairing_host.pause_external_session_activation_for_tests();
    let activation = std::thread::spawn({
        let pairing_host = pairing_host.clone();
        move || futures::executor::block_on(pairing_host.activate_external_session(&stale_blob))
    });
    futures::executor::block_on(activation_entered)
        .expect("external activation reached the installation fence");

    pairing_host.notify_session_store_changed();
    resume_activation
        .send(())
        .expect("external activation remains in flight");
    activation
        .join()
        .expect("external activation thread panicked")
        .expect("superseded external activation completes");

    wait_until(
        || host.test_session_state().current() == Some(persisted.clone()),
        "store reconciliation did not preserve the persisted replacement",
    );
    assert_eq!(host.test_session_state().current(), Some(persisted));
}

#[test]
fn disconnect_during_external_activation_prevents_stale_reinstallation() {
    let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
        Arc::new(StubPlatform::default()),
        test_spawner(),
    );
    let stale_blob = crate::host_logic::session::encode_persisted_session(&sso_session_info());
    let (activation_entered, resume_activation) =
        pairing_host.pause_external_session_activation_for_tests();
    let activation = std::thread::spawn({
        let pairing_host = pairing_host.clone();
        move || futures::executor::block_on(pairing_host.activate_external_session(&stale_blob))
    });
    futures::executor::block_on(activation_entered)
        .expect("external activation reached the installation fence");

    futures::executor::block_on(host.disconnect());
    resume_activation
        .send(())
        .expect("external activation remains in flight");
    activation
        .join()
        .expect("external activation thread panicked")
        .expect("superseded external activation completes");

    assert!(
        host.test_session_state().current().is_none(),
        "disconnect must win over the earlier external activation"
    );
}

#[test]
fn stored_session_activation_resolves_after_connected_installation() {
    let stored = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &stored,
        )),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    futures::executor::block_on(pairing_host.activate_stored_session())
        .expect("valid stored session activates");

    assert_eq!(host.test_session_state().current(), Some(stored.clone()));
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Connected(connected_session_ui_info(&stored))]
    );
}

#[test]
fn stored_session_activation_rejects_invalid_blob_and_disconnects() {
    let session_clears = Arc::new(Mutex::new(0));
    let platform = Arc::new(StubPlatform {
        session_blob: Some(vec![0xff]),
        session_clears: session_clears.clone(),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform, test_spawner());
    install_pairing_session(&host, sso_session_info());

    let error = futures::executor::block_on(pairing_host.activate_stored_session())
        .expect_err("invalid stored session is rejected");

    assert!(error.starts_with("invalid stored auth session:"));
    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        1
    );
}

/// An untagged blob restores and the slot is rewritten in the written form, so
/// a host upgrades its stored session by activating once rather than by pairing
/// again. `SessionInfo::encode` is the untagged eight-field layout; the canary in
/// `host_logic::session` is what keeps that true.
#[test]
fn activating_an_untagged_stored_session_restores_it_and_rewrites_the_slot() {
    let stored = sso_session_info();
    let untagged = stored.encode();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(untagged.clone()),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    futures::executor::block_on(pairing_host.activate_stored_session())
        .expect("an untagged stored session activates");

    assert_eq!(host.test_session_state().current(), Some(stored.clone()));
    let written = crate::host_logic::session::encode_persisted_session(&stored);
    assert_ne!(
        untagged, written,
        "the fixture must not already be in the written form, or this proves nothing"
    );
    assert_eq!(
        platform
            .session_writes
            .lock()
            .expect("session write list mutex poisoned")
            .last(),
        Some(&written),
        "the slot still holds the untagged blob, so it would be re-read on every start"
    );
}

#[test]
fn session_store_sync_restores_valid_blob_from_tick() {
    let stored = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &stored,
        )),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || host.test_session_state().current() == Some(stored.clone()),
        "session store sync did not restore valid blob",
    );

    assert_eq!(host.test_session_state().current(), Some(stored.clone()));
    let expected_auth_states = vec![AuthState::Connected(connected_session_ui_info(&stored))];
    wait_until(
        || {
            *platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                == expected_auth_states
        },
        "session store sync did not broadcast connected auth state",
    );
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        expected_auth_states
    );
}

#[test]
fn session_store_sync_announces_a_signed_out_boot() {
    let platform = Arc::new(StubPlatform::default());
    let (_host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());

    wait_until(
        || {
            !platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        },
        "boot reconcile did not report the signed-out state",
    );
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Disconnected]
    );
}

#[test]
fn session_store_sync_announces_a_restored_boot_once() {
    let stored = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &stored,
        )),
        ..Default::default()
    });
    let (_host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());

    wait_until(
        || {
            !platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        },
        "boot reconcile did not report the restored session",
    );
    futures::executor::block_on(pairing_host.activate_stored_session())
        .expect("valid stored session activates");
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Connected(connected_session_ui_info(&stored))]
    );
}

#[test]
fn session_store_sync_stays_silent_on_an_unchanged_tick() {
    let stored = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &stored,
        )),
        ..Default::default()
    });
    let (_host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || {
            !platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        },
        "boot reconcile did not report the restored session",
    );

    pairing_host.notify_session_store_changed();
    wait_until(
        || pairing_host.session_store_change_ticks_for_tests() == 1,
        "session store sync did not process the change tick",
    );

    // The store still holds the same session, so the tick is not a
    // transition and must not repeat the opening state.
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Connected(connected_session_ui_info(&stored))]
    );
}

#[test]
fn session_store_sync_replaces_valid_blob_and_broadcasts_connected() {
    let mut replacement = sso_session_info();
    replacement.public_key = [0x44; 32];
    let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
        Arc::new(StubPlatform {
            session_blob: Some(crate::host_logic::session::encode_persisted_session(
                &replacement,
            )),
            ..Default::default()
        }),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());
    let mut statuses = host.test_session_state().subscribe();
    let _ = futures::executor::block_on(statuses.next());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());

    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Connected
        )
    );
    assert_eq!(host.test_session_state().current(), Some(replacement));
}

#[test]
fn session_store_sync_clears_invalid_blob() {
    let platform = Arc::new(StubPlatform {
        session_blob: Some(vec![0xff]),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    install_pairing_session(&host, sso_session_info());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || host.test_session_state().current().is_none(),
        "session store sync did not clear invalid blob",
    );

    assert!(host.test_session_state().current().is_none());
    // `set_session` bypasses the auth state cell, so the clear is not a
    // transition; the boot tick's announcement is the only emission.
    wait_until(
        || {
            !platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        },
        "boot reconcile did not report the cleared session",
    );
    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![AuthState::Disconnected]
    );
}

#[test]
fn session_store_sync_clears_unreadable_blob() {
    let session_clears = Arc::new(Mutex::new(0));
    let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
        Arc::new(StubPlatform {
            session_error: Some("storage unavailable"),
            session_clears: session_clears.clone(),
            ..Default::default()
        }),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || *session_clears.lock().unwrap() == 1,
        "session store sync did not clear unreadable blob",
    );

    assert!(host.test_session_state().current().is_none());
    assert_eq!(*session_clears.lock().unwrap(), 1);
}

/// A persistently failing read clears the backing store once at boot.
/// Further clears require explicit host notifications.
#[test]
fn session_store_sync_clears_once_on_initial_persistent_read_error() {
    let session_clears = Arc::new(Mutex::new(0));
    let (host, pairing_host) = ProductRuntimeHost::new_compat_with_pairing(
        Arc::new(StubPlatform {
            session_error: Some("storage unavailable"),
            session_clears: session_clears.clone(),
            ..Default::default()
        }),
        test_spawner(),
    );
    install_pairing_session(&host, sso_session_info());

    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());

    wait_until(
        || *session_clears.lock().unwrap() == 1,
        "clear_stored_session was never called",
    );
    assert_eq!(*session_clears.lock().unwrap(), 1);
    assert!(host.test_session_state().current().is_none());
}

#[test]
fn disconnect_submits_disconnected_message_best_effort() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    let session = sso_session_info();
    install_pairing_session(&host, session.clone());

    futures::executor::block_on(host.disconnect());

    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *platform
            .session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        1
    );
    let message = submitted_remote_message(&platform, &session);
    assert_eq!(message.message_id.len(), 8, "opaque nanoid message id");
    assert!(matches!(
        message.data,
        RemoteMessageData::V1(v1::RemoteMessage::Disconnected)
    ));
}

#[test]
fn pairing_logout_clears_session_and_bootstrap_identity() {
    let platform = Arc::new(StubPlatform::default());
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    install_pairing_session(&host, sso_session_info());
    {
        let mut storage = platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned");
        storage.insert(
            core_storage_test_key(CoreStorageKey::PairingDeviceIdentity),
            vec![1, 2, 3],
        );
        storage.insert(
            core_storage_test_key(CoreStorageKey::LastProcessedPairingStatement),
            vec![4, 5, 6],
        );
    }

    futures::executor::block_on(pairing_host.logout_and_reset_pairing()).unwrap();

    assert!(host.test_session_state().current().is_none());
    let storage = platform
        .local_storage
        .lock()
        .expect("local storage mutex poisoned");
    assert!(!storage.contains_key(&core_storage_test_key(
        CoreStorageKey::PairingDeviceIdentity
    )));
    assert!(!storage.contains_key(&core_storage_test_key(
        CoreStorageKey::LastProcessedPairingStatement
    )));
}

#[test]
fn disconnect_clears_session_store_and_broadcasts_disconnected() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());
    install_pairing_session(&host, sso_session_info());
    platform
        .local_storage
        .lock()
        .expect("local storage mutex poisoned")
        .insert(
            core_storage_test_key(CoreStorageKey::PairingDeviceIdentity),
            vec![1, 2, 3],
        );
    let mut statuses = host.test_session_state().subscribe();
    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Connected
        )
    );

    futures::executor::block_on(host.disconnect());

    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *platform
            .session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        1
    );
    assert!(
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .contains_key(&core_storage_test_key(
                CoreStorageKey::PairingDeviceIdentity
            )),
        "logout may leave the old pairing identity in storage; the next login rotates it before presenting QR"
    );
    assert_eq!(
        futures::executor::block_on(statuses.next()).unwrap(),
        HostAccountConnectionStatusSubscribeItem::V1(
            v01::HostAccountConnectionStatusSubscribeItem::Disconnected
        )
    );
    // `set_session` bypasses the auth state cell, so the cell never left
    // `Disconnected` and the logout emits nothing new. Only a session
    // activation announces an unchanged state.
    assert!(
        platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned")
            .is_empty()
    );
}

#[test]
fn disconnect_emits_disconnected_auth_state_after_store_sync_connected() {
    let stored = sso_session_info();
    let platform = Arc::new(StubPlatform {
        session_blob: Some(crate::host_logic::session::encode_persisted_session(
            &stored,
        )),
        ..Default::default()
    });
    let (host, pairing_host) =
        ProductRuntimeHost::new_compat_with_pairing(platform.clone(), test_spawner());
    pairing_host
        .clone()
        .start_session_store_sync_for_tests(test_spawner());
    wait_until(
        || {
            platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .len()
                == 1
        },
        "session store sync did not emit connected auth state",
    );

    futures::executor::block_on(host.disconnect());

    assert_eq!(
        *platform
            .auth_states
            .lock()
            .expect("auth state list mutex poisoned"),
        vec![
            AuthState::Connected(connected_session_ui_info(&stored)),
            AuthState::Disconnected,
        ]
    );
}

#[test]
fn disconnect_tolerates_repeated_logout_when_already_disconnected() {
    let platform = Arc::new(StubPlatform::default());
    let host = ProductRuntimeHost::new_compat(platform.clone(), test_spawner());

    futures::executor::block_on(host.disconnect());
    futures::executor::block_on(host.disconnect());

    assert!(host.test_session_state().current().is_none());
    assert_eq!(
        *platform
            .session_clears
            .lock()
            .expect("session clear counter mutex poisoned"),
        2
    );
    assert!(platform.sent_rpc.lock().unwrap().is_empty());
}

#[test]
fn permissions_grants_and_caches() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera);
    let response =
        futures::executor::block_on(host.request_device_permission(&cx, request)).unwrap();
    let HostDevicePermissionResponse::V1(inner) = response;
    assert!(inner.granted);
}

#[test]
fn feature_supported_encodes_response_to_known_bytes() {
    let host = ProductRuntimeHost::new_compat(stub_platform(), test_spawner());
    let cx = CallContext::default();
    let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
        genesis_hash: vec![0u8; 32],
    });
    let response = futures::executor::block_on(host.feature_supported(&cx, request)).unwrap();
    // [V1 variant=0][supported=1]
    assert_eq!(response.encode(), vec![0x00, 0x01]);
}

mod signing;

/// The pairing authority's cross-product gate, driven directly.
///
/// `pairing_host.rs` carried no `#[test]` at all: every grant test drove the
/// signing role, and the e2e drives the signing-host CLI. Replacing the body of
/// `PairingHost::require_ring_vrf_key_access` with `Ok(())`, which lets any
/// paired peer reach any product's ring-VRF key by naming it, left the entire
/// package green. That is the exact threat #655 gives as the reason the authority must
/// adjudicate for itself rather than trust a relayed verdict, so it cannot be
/// the one path with no coverage.
///
/// Driven at the authority, which is where a pairing-wire request arrives:
/// `sso_responder` hands `calling_product_id` and `key_handle` straight here,
/// both decoded from the peer's message.
#[test]
fn the_pairing_authority_refuses_a_foreign_ring_vrf_key_without_a_grant() {
    let (host_config, product) = runtime_config("dim2.dot");
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host.clone(), product);
    install_pairing_session(&host, session_info());
    let session = pairing_host
        .current_session()
        .expect("the pairing host has an active session");

    let proof = futures::executor::block_on(ProductAuthority::create_proof(
        &*pairing_host,
        &CallContext::default(),
        &session,
        crate::host_logic::sso::messages::ProductRequest {
            calling_product_id: "dim2.dot".to_string(),
            payload: v01::HostAccountCreateProofRequest {
                key_handle: v01::ProductAccountId {
                    dot_ns_identifier: "peopl.dot".to_string(),
                    derivation_index: v01::DerivationIndex::Index(0),
                },
                context: v01::ProductProofContext {
                    product_id: "dim2.dot".to_string(),
                    suffix: v01::DerivationIndex::Index(0),
                },
                ring_location: ring_location_fixture(),
                message: b"prove me".to_vec(),
            },
        },
    ));
    assert_eq!(
        proof.err(),
        Some(RingVrfError::NotAllowlisted),
        "the pairing authority must refuse a foreign key that no manifest granted"
    );

    let signed = futures::executor::block_on(ProductAuthority::ring_vrf_sign(
        &*pairing_host,
        &CallContext::default(),
        &session,
        crate::host_logic::sso::messages::ProductRequest {
            calling_product_id: "dim2.dot".to_string(),
            payload: v01::HostAccountRingVrfSignRequest {
                key_handle: v01::ProductAccountId {
                    dot_ns_identifier: "peopl.dot".to_string(),
                    derivation_index: v01::DerivationIndex::Index(0),
                },
                message: b"sign me".to_vec(),
            },
        },
    ));
    assert_eq!(
        signed.err(),
        Some(RingVrfError::NotAllowlisted),
        "and the same on the signing method, which arrives through the same door"
    );
}

/// The grant lookup obeys the caller's deadline.
///
/// It can reach dotNS on the Asset Hub, which is several sequential chain
/// operations each bounded only by `OPERATION_TIMEOUT` (10s). Run before
/// `remote_authority_call` that cost sat outside the caller's deadline and
/// ignored a cancel, so a product asking for a short timeout could wait far
/// longer with no way to stop it. This pins that it now returns on the deadline:
/// the stub answers no RPC, so an unscoped lookup would stall for the full
/// operation timeout instead.
#[test]
fn a_grant_lookup_obeys_the_callers_deadline() {
    let (host_config, product) = runtime_config("dim2.dot");
    let platform: Arc<dyn Platform> = stub_platform();
    let services = RuntimeServices::new(
        platform.clone(),
        host_config.host.host_info.clone(),
        host_config.people_chain_genesis_hash,
        host_config.bulletin_chain_genesis_hash,
        host_config.asset_hub_chain_genesis_hash,
        test_spawner(),
    );
    let pairing_host = PairingHost::new(services.clone(), host_config);
    let adapters = crate::host_core::ConnectionAdapters::from_services(&services);
    let host = ProductRuntimeHost::from_services(services, adapters, pairing_host, product);
    install_pairing_session(&host, session_info());

    let mut cx = CallContext::default();
    cx.set_timeout(Duration::from_millis(1));
    let started = std::time::Instant::now();
    let result = futures::executor::block_on(
        host.create_account_proof(&cx, create_proof_request("peopl.dot")),
    );
    let elapsed = started.elapsed();

    // The uniform refusal, not a transport error carrying a reason. Left to
    // `remote_authority_call`, a deadline that expires during the lookup answers
    // `Unknown { reason }` while an already-cached target that grants nothing
    // answers `NotAllowlisted` at once, so the error tag alone would tell a
    // caller which targets this device has resolved before, which is the
    // enumeration the denial read was moved after the manifest to prevent.
    assert!(
        matches!(
            result.as_ref().err(),
            Some(CallError::Domain(HostAccountCreateProofError::V1(
                v01::HostAccountCreateProofError::NotAllowlisted
            )))
        ),
        "a lookup that runs out of time must answer the uniform refusal, got {:?}",
        result.as_ref().err()
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the grant lookup must be bounded by the caller's deadline, not by the \
         dotNS operation timeout; took {elapsed:?}"
    );
}

/// Subnames of one product share one cached manifest, because they are one
/// dotNS node holding one document.
///
/// The cache used to be keyed by the full product id while the document is
/// resolved by the bare label, so `app.peopl.dot` and `worker.peopl.dot` each
/// drove a fresh chain resolution and a fresh durable write for a document the
/// host already held, with nothing bounding how many spellings a caller could
/// name. On the pairing wire that id comes from the peer.
#[test]
fn subnames_of_one_product_share_one_cached_manifest() {
    let platform = stub_platform();
    cache_manifest(&platform, "peopl.dot", r#"{"unknown":["storage"]}"#, 0);
    seed_owner_value(&platform, "peopl.dot");
    let host = ProductRuntimeHost::new_compat(platform, test_spawner());

    // Seeded once under `peopl.dot`; every executable beneath it reads it.
    for spelling in ["peopl.dot", "app.peopl.dot", "worker.peopl.dot"] {
        assert!(
            read_storage(&host, Some(spelling), "k").is_ok(),
            "{spelling} must resolve the one manifest cached for its product"
        );
    }
}

/// A cancellation the host raised itself must never reach the wire as
/// `CallError::Cancelled`.
///
/// That variant is appended last in `CallError`, so a product built before it
/// existed cannot decode it. It is reserved for a call the peer withdrew with
/// a `Cancel` frame, because a peer that never sends one never has to decode
/// the answer. An internal timeout is not that: it has always reported through
/// the method's own error type and has to keep doing so.
///
/// Every mapper that turns an [`AuthorityError`] into a `CallError` is checked
/// here, because the tempting simplification is to map `Cancelled` to the new
/// variant in one of them and leave the rest alone.
#[test]
fn an_internal_cancellation_never_becomes_the_cancelled_variant() {
    use super::{
        account_get_authority_error, signing_call_error, transaction_call_error, vrf_call_error,
    };
    use crate::runtime::authority::{AuthorityCancelError, AuthorityError};
    use truapi::CancellationReason;
    use truapi::versioned::signing::{HostCreateTransactionError, HostSignRawError};

    fn assert_domain<E: core::fmt::Debug>(mapper: &str, reason: &str, err: CallError<E>) {
        assert!(
            matches!(err, CallError::Domain(_)),
            "{mapper} must report an internal cancellation ({reason}) through its own \
             domain error, got {err:?}"
        );
    }

    let reasons = [
        ("explicit", CancellationReason::Cancelled),
        (
            "timeout",
            CancellationReason::TimedOut {
                timeout: core::time::Duration::from_secs(30),
            },
        ),
    ];

    for (reason_name, reason) in reasons {
        let cancelled =
            || AuthorityError::Cancelled(AuthorityCancelError::new("p:1", reason.clone()));

        assert_domain("vrf_call_error", reason_name, vrf_call_error(cancelled()));
        assert_domain(
            "account_get_authority_error",
            reason_name,
            account_get_authority_error(cancelled()),
        );
        assert_domain(
            "signing_call_error",
            reason_name,
            signing_call_error(HostSignRawError::V1, cancelled()),
        );
        assert_domain(
            "transaction_call_error",
            reason_name,
            transaction_call_error(HostCreateTransactionError::V1, cancelled()),
        );
    }
}
