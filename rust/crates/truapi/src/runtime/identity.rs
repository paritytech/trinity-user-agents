//! dotNS identity lookup used to resolve usernames for a paired session.
//!
//! Usernames live in the dotNS contracts on Asset Hub. The gateway pallet
//! storage anchors the `DotnsPopController`. The protocol registry locates the
//! `StoreFactory`. The account's labels come from its `LabelStore` on the warm
//! path. On the cold path they come from its pending claim on the controller,
//! covering gateway-minted names before the user settles their store. When
//! neither yields a lite username, the gateway pallet's own
//! `DotnsGateway.AccountNames` record does.
//!
//! All reads run over one `chainHead_v1` follow: storage reads and
//! `ReviveApi_call` dry-runs. No chain metadata is needed.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::dotns_gateway::{
    DotnsIdentity, discover_pop_controller, resolve_identity,
};
use crate::host_logic::session::SessionInfo;
use crate::runtime::dotns_lookup::DotnsLookup;
use crate::session_usernames::SessionUsernames;

use futures::{FutureExt, pin_mut};
use tracing::{debug, instrument, warn};

/// Budget for the whole username resolution of one session: every attempt for
/// the identity account plus the root-key fallback share it, so a slow or dead
/// endpoint delays session installation by at most this long.
const LOOKUP_BUDGET: Duration = Duration::from_secs(45);

/// Fills in missing usernames by querying the dotNS contracts on Asset Hub.
/// Returns the session unchanged when it already carries a username. Also
/// returns it unchanged when no Asset Hub is configured.
#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
pub async fn resolve_session_identity_with_chain(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    mut session: SessionInfo,
) -> SessionInfo {
    if session.has_username() || asset_hub_chain_genesis_hash == [0; 32] {
        return session;
    }

    {
        let budget = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
        pin_mut!(budget);
        let resolve = async {
            let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
            if lookup_and_apply(
                chain,
                asset_hub_chain_genesis_hash,
                preferred_account,
                &mut session,
                "identity",
            )
            .await
                == LookupOutcome::NoRecord
                && preferred_account != session.public_key
            {
                let public_key = session.public_key;
                lookup_and_apply(
                    chain,
                    asset_hub_chain_genesis_hash,
                    public_key,
                    &mut session,
                    "root identity",
                )
                .await;
            }
        }
        .fuse();
        pin_mut!(resolve);
        futures::select! {
            () = resolve => {}
            () = budget => {
                warn!(
                    "dotNS username resolution ran out of budget; the session installs without one"
                );
            }
        }
    }

    session
}

/// Maximum lookup attempts per account on transient failure. The first attempt
/// warms the Asset Hub connection, cached per genesis. A retry after a cold-start
/// timeout therefore usually resolves immediately.
const IDENTITY_LOOKUP_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LookupOutcome {
    /// A username record was found and applied.
    Applied,
    /// The account has no dotNS labels. Definitive, not worth a retry.
    NoRecord,
    /// The lookup failed transiently after exhausting retries.
    Failed,
}

/// Looks up `account`'s dotNS identity and applies any usernames to `session`.
/// Transient failures are retried against the warmed connection.
async fn lookup_and_apply(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    account: [u8; 32],
    session: &mut SessionInfo,
    label: &str,
) -> LookupOutcome {
    for attempt in 1..=IDENTITY_LOOKUP_MAX_ATTEMPTS {
        match lookup_dotns_identity(chain, asset_hub_chain_genesis_hash, account).await {
            Ok(Some(identity)) => {
                debug!(
                    account = %hex::encode(account),
                    lite_username = identity.lite_username.as_deref().unwrap_or(""),
                    full_username = identity.full_username.as_deref().unwrap_or(""),
                    "dotNS {label} lookup found username"
                );
                session.apply_usernames(identity.lite_username, identity.full_username);
                return LookupOutcome::Applied;
            }
            Ok(None) => {
                debug!(
                    account = %hex::encode(account),
                    "dotNS {label} lookup found no labels"
                );
                return LookupOutcome::NoRecord;
            }
            Err(reason) => {
                warn!(
                    account = %hex::encode(account),
                    attempt,
                    %reason,
                    "dotNS {label} lookup failed"
                );
            }
        }
    }
    LookupOutcome::Failed
}

/// Resolves `account_id`'s usernames from dotNS at a fresh Asset Hub head.
/// Each step carries the lookup transport's own step timeout; the caller's
/// [`LOOKUP_BUDGET`] bounds the whole resolution. Returns `None` when the
/// gateway is not deployed. Also returns `None` when the account has no
/// username.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_dotns_identity(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    account_id: [u8; 32],
) -> Result<Option<DotnsIdentity>, String> {
    let lookup = async {
        let mut lookup = DotnsLookup::pinned_to_best_block(
            chain,
            asset_hub_chain_genesis_hash,
            &format!("identity:{}", hex::encode(account_id)),
        )
        .await?;
        let Some(controller) = discover_pop_controller(&mut lookup).await? else {
            return Ok(None);
        };
        let identity = resolve_identity(&mut lookup, &controller, &account_id).await?;
        Ok((identity != DotnsIdentity::default()).then_some(identity))
    }
    .fuse();
    pin_mut!(lookup);
    lookup.await
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    //! The in-core lookup drives every dotNS read over one `chainHead_v1`
    //! follow. This scripts the Asset Hub node end of that follow: storage
    //! items for the pallet keys and `ReviveApi_call` outputs for the contract
    //! views, so the whole chain from follow to classified usernames runs
    //! without a network.

    use super::*;
    use crate::chain_runtime::{RuntimeChainProvider, RuntimeFailure};
    use crate::host_logic::dotns_gateway::{
        VIEW_CALL_ORIGIN, account_names_key, account_to_h160, decode_string,
        dispatcher_address_key, lite_label_owner_key, selector,
    };
    use crate::platform::JsonRpcConnection;
    use crate::subscription::thread_per_subscription_spawner;
    use async_trait::async_trait;
    use futures::StreamExt;
    use futures::channel::mpsc;
    use futures::stream::BoxStream;
    use parity_scale_codec::{Compact, Decode, Encode};
    use serde_json::{Value as JsonValue, json};
    use std::sync::{Arc, Mutex};

    const FOLLOW_ID: &str = "ah-follow";
    const BEST_HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
    const DISPATCHER: [u8; 20] = [0xd1; 20];
    const CONTROLLER: [u8; 20] = [0xc0; 20];
    const REGISTRY: [u8; 20] = [0x9e; 20];
    const FACTORY: [u8; 20] = [0xfa; 20];
    const STORE: [u8; 20] = [0x57; 20];
    const ACCOUNT: [u8; 32] = [0xaa; 32];
    /// Onboarded before dotted labels: its pending claim is the undotted
    /// `legacy01`, which `isPopIssued` denies, and the gateway pallet records
    /// `legacy.01`.
    const LEGACY_ACCOUNT: [u8; 32] = [0xbb; 32];

    fn abi_word(value: u64) -> [u8; 32] {
        let mut word = [0u8; 32];
        word[24..].copy_from_slice(&value.to_be_bytes());
        word
    }

    fn abi_address(address: &[u8; 20]) -> Vec<u8> {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(address);
        word.to_vec()
    }

    /// ABI string tail: a length word plus padded bytes.
    fn abi_string_tail(value: &str) -> Vec<u8> {
        let mut out = abi_word(value.len() as u64).to_vec();
        out.extend_from_slice(value.as_bytes());
        out.resize(out.len().div_ceil(32) * 32, 0);
        out
    }

    /// A single `string` return value.
    fn abi_string(value: &str) -> Vec<u8> {
        [abi_word(32).to_vec(), abi_string_tail(value)].concat()
    }

    /// A `string[]` return value.
    fn abi_string_array(values: &[&str]) -> Vec<u8> {
        let tails: Vec<Vec<u8>> = values.iter().map(|v| abi_string_tail(v)).collect();
        let mut out = abi_word(32).to_vec();
        out.extend_from_slice(&abi_word(values.len() as u64));
        let mut offset = 32 * values.len();
        for tail in &tails {
            out.extend_from_slice(&abi_word(offset as u64));
            offset += tail.len();
        }
        for tail in tails {
            out.extend_from_slice(&tail);
        }
        out
    }

    /// A `(string label, uint64 mintedAt)[]` return value.
    fn abi_pending_claims(claims: &[(&str, u64)]) -> Vec<u8> {
        let structs: Vec<Vec<u8>> = claims
            .iter()
            .map(|(label, minted_at)| {
                [
                    abi_word(64).to_vec(),
                    abi_word(*minted_at).to_vec(),
                    abi_string_tail(label),
                ]
                .concat()
            })
            .collect();
        let mut out = abi_word(32).to_vec();
        out.extend_from_slice(&abi_word(claims.len() as u64));
        let mut offset = 32 * claims.len();
        for element in &structs {
            out.extend_from_slice(&abi_word(offset as u64));
            offset += element.len();
        }
        for element in structs {
            out.extend_from_slice(&element);
        }
        out
    }

    /// `ReviveApi_call` output carrying successful return `data`.
    fn contract_result(data: &[u8]) -> Vec<u8> {
        contract_result_with_flags(0, data)
    }

    /// `ReviveApi_call` output for a call that reverted.
    fn reverted_contract_result() -> Vec<u8> {
        contract_result_with_flags(1, &[])
    }

    /// `flags` is `ReturnFlags`; bit 0 marks a revert.
    fn contract_result_with_flags(flags: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for _ in 0..4 {
            Compact(7u64).encode_to(&mut out);
        }
        for _ in 0..2 {
            (1u8, 0u128).encode_to(&mut out);
        }
        0u128.encode_to(&mut out);
        out.push(0x00);
        flags.encode_to(&mut out);
        data.encode_to(&mut out);
        out
    }

    /// The scripted contract side: `(dest, selector)` → return data.
    fn view_output(dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
        let sel: [u8; 4] = input[..4].try_into().unwrap();
        // Discovery probes `protocolRegistry()` first and the dispatcher reverts it; that
        // revert is how the two contracts are told apart, so the mock reproduces it.
        if *dest == DISPATCHER && sel == selector("protocolRegistry()") {
            return reverted_contract_result();
        }
        let data = match (*dest, sel) {
            (DISPATCHER, s) if s == selector("TARGET()") => abi_address(&CONTROLLER),
            (CONTROLLER, s) if s == selector("pendingClaims(address,uint256,uint256)") => {
                // First page for a mapped account.
                assert_eq!(&input[36..68], &abi_word(0));
                assert_eq!(&input[68..100], &abi_word(16));
                if input[4..36] == abi_address(&account_to_h160(&ACCOUNT)) {
                    // Minted at the epoch: pending claims never lapse.
                    abi_pending_claims(&[("alice.01", 1)])
                } else {
                    assert_eq!(
                        &input[4..36],
                        abi_address(&account_to_h160(&LEGACY_ACCOUNT)).as_slice()
                    );
                    abi_pending_claims(&[("legacy01", 1)])
                }
            }
            (CONTROLLER, s) if s == selector("isPopIssued(string)") => {
                // Every label but the legacy undotted claim came through the gateway.
                let label = decode_string(&input[4..]).unwrap();
                abi_word((label != "legacy01").into()).to_vec()
            }
            (CONTROLLER, s) if s == selector("protocolRegistry()") => abi_address(&REGISTRY),
            (REGISTRY, s) if s == selector("get(bytes32)") => abi_address(&FACTORY),
            (REGISTRY, s) if s == selector("tld()") => abi_string(".paseo"),
            (FACTORY, s) if s == selector("getLabelStore(address)") => {
                if input[16..36] == account_to_h160(&ACCOUNT) {
                    abi_address(&STORE)
                } else {
                    assert_eq!(&input[16..36], account_to_h160(&LEGACY_ACCOUNT));
                    abi_address(&[0; 20])
                }
            }
            (STORE, s) if s == selector("getLabels(uint256,uint256)") => {
                abi_string_array(&["myproject.paseo", "app.myproject.paseo"])
            }
            (dest, sel) => panic!(
                "unscripted view {} on 0x{}",
                hex::encode(sel),
                hex::encode(dest)
            ),
        };
        contract_result(&data)
    }

    /// The scripted pallet storage: the dispatcher address, and the gateway's
    /// own record of the legacy account's dotted lite name.
    fn storage_value(key: &[u8]) -> Option<Vec<u8>> {
        if key == dispatcher_address_key() {
            return Some(DISPATCHER.to_vec());
        }
        if key == account_names_key(&LEGACY_ACCOUNT) {
            let lite = (b"legacy.01".to_vec(), None::<[u8; 65]>);
            return Some((Some(lite), None::<(Vec<u8>, Option<[u8; 65]>)>).encode());
        }
        if key == lite_label_owner_key(b"legacy.01") {
            return Some(LEGACY_ACCOUNT.to_vec());
        }
        None
    }

    struct ScriptedAssetHub {
        sent: Arc<Mutex<Vec<String>>>,
        follow_with_runtime: Arc<Mutex<Option<bool>>>,
        sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
        receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
        next_operation: Arc<Mutex<u64>>,
    }

    impl ScriptedAssetHub {
        fn new() -> Self {
            let (sender, receiver) = mpsc::unbounded();
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                follow_with_runtime: Arc::new(Mutex::new(None)),
                sender: Arc::new(Mutex::new(Some(sender))),
                receiver: Arc::new(Mutex::new(Some(receiver))),
                next_operation: Arc::new(Mutex::new(0)),
            }
        }

        fn share(&self) -> Self {
            Self {
                sent: self.sent.clone(),
                follow_with_runtime: self.follow_with_runtime.clone(),
                sender: self.sender.clone(),
                receiver: self.receiver.clone(),
                next_operation: self.next_operation.clone(),
            }
        }

        fn frames(&self, request: &str) -> Vec<String> {
            let request: JsonValue = serde_json::from_str(request).unwrap();
            let id = request.get("id").cloned().unwrap_or(JsonValue::Null);
            let method = request["method"].as_str().unwrap();
            let response = |result: JsonValue| {
                json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
            };
            let follow_event = |result: JsonValue| {
                json!({
                    "jsonrpc": "2.0",
                    "method": "chainHead_v1_followEvent",
                    "params": {"subscription": FOLLOW_ID, "result": result}
                })
                .to_string()
            };
            let mut next_operation = self.next_operation.lock().unwrap();
            match method {
                "chainHead_v1_follow" => {
                    *self.follow_with_runtime.lock().unwrap() = request["params"][0].as_bool();
                    vec![
                        response(json!(FOLLOW_ID)),
                        follow_event(json!({
                            "event": "initialized",
                            "finalizedBlockHashes": [BEST_HASH],
                            "finalizedBlockRuntime": null
                        })),
                        follow_event(json!({
                            "event": "bestBlockChanged",
                            "bestBlockHash": BEST_HASH
                        })),
                    ]
                }
                "chainHead_v1_storage" => {
                    *next_operation += 1;
                    let operation_id = format!("storage-{}", *next_operation);
                    let key = request["params"][2][0]["key"].as_str().unwrap();
                    let key_bytes = hex::decode(key.trim_start_matches("0x")).unwrap();
                    let mut frames = vec![response(
                        json!({"result": "started", "operationId": operation_id}),
                    )];
                    if let Some(value) = storage_value(&key_bytes) {
                        frames.push(follow_event(json!({
                            "event": "operationStorageItems",
                            "operationId": operation_id,
                            "items": [{"key": key, "value": format!("0x{}", hex::encode(value))}]
                        })));
                    }
                    frames.push(follow_event(json!({
                        "event": "operationStorageDone",
                        "operationId": operation_id
                    })));
                    frames
                }
                "chainHead_v1_call" => {
                    *next_operation += 1;
                    let operation_id = format!("call-{}", *next_operation);
                    assert_eq!(request["params"][2].as_str(), Some("ReviveApi_call"));
                    let args = hex::decode(
                        request["params"][3]
                            .as_str()
                            .unwrap()
                            .trim_start_matches("0x"),
                    )
                    .unwrap();
                    // origin[32] ‖ dest[20] ‖ value u128 ‖ None ‖ None ‖ Vec(input).
                    assert_eq!(&args[..32], VIEW_CALL_ORIGIN.as_slice());
                    assert_eq!(
                        &args[52..70],
                        &[0u8; 18],
                        "zero value, no gas or deposit limit"
                    );
                    let dest: [u8; 20] = args[32..52].try_into().unwrap();
                    let input = Vec::<u8>::decode(&mut &args[70..]).unwrap();
                    vec![
                        response(json!({"result": "started", "operationId": operation_id})),
                        follow_event(json!({
                            "event": "operationCallDone",
                            "operationId": operation_id,
                            "output": format!("0x{}", hex::encode(view_output(&dest, &input)))
                        })),
                    ]
                }
                "chainHead_v1_unpin" | "chainHead_v1_unfollow" => vec![response(JsonValue::Null)],
                other => panic!("unscripted method {other}"),
            }
        }
    }

    struct ScriptedConnection {
        provider: ScriptedAssetHub,
        receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    }

    impl JsonRpcConnection for ScriptedConnection {
        fn send(&self, request: String) {
            self.provider.sent.lock().unwrap().push(request.clone());
            let frames = self.provider.frames(&request);
            if let Some(sender) = self.provider.sender.lock().unwrap().as_ref() {
                for frame in frames {
                    sender.unbounded_send(frame).unwrap();
                }
            }
        }

        fn responses(&self) -> BoxStream<'static, String> {
            self.receiver
                .lock()
                .unwrap()
                .take()
                .expect("responses called once")
                .boxed()
        }

        fn close(&self) {
            self.provider.sender.lock().unwrap().take();
        }
    }

    #[async_trait]
    impl RuntimeChainProvider for ScriptedAssetHub {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
            Ok(Arc::new(ScriptedConnection {
                receiver: Mutex::new(self.receiver.lock().unwrap().take()),
                provider: self.share(),
            }))
        }
    }

    #[test]
    fn in_core_lookup_resolves_usernames_over_one_runtime_follow() {
        let provider = Arc::new(ScriptedAssetHub::new());
        let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
        let session = SessionInfo {
            public_key: [0x11; 32],
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(ACCOUNT),
            identity_chat_private_key: None,
            device_enc_public_key: None,
            lite_username: None,
            full_username: None,
        };

        let resolved = futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], session,
        ));

        // The pending claim yields the lite name; the settled store yields the
        // full name with the network TLD stripped and the subname dropped.
        assert_eq!(resolved.lite_username.as_deref(), Some("alice.01"));
        assert_eq!(resolved.full_username.as_deref(), Some("myproject"));
        // `chainHead_v1_call` is only served on follows opened with runtime.
        assert_eq!(
            *provider.follow_with_runtime.lock().unwrap(),
            Some(true),
            "the identity follow must be opened withRuntime=true or every view fails"
        );
        let calls = provider
            .sent
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.contains("chainHead_v1_call"))
            .count();
        // protocolRegistry (reverts on the dispatcher), TARGET, pendingClaims,
        // protocolRegistry, get(storeFactory), getLabelStore, tld, one short
        // getLabels page, then one isPopIssued per label (alice.01,
        // myproject). A repointed chain resolves on the first probe and needs
        // nine.
        assert_eq!(
            calls, 10,
            "the discovery, label and provenance chain is exactly ten views on a dispatcher chain"
        );
    }

    #[test]
    fn a_legacy_undotted_claim_resolves_through_the_gateway_record() {
        let provider = Arc::new(ScriptedAssetHub::new());
        let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
        let session = SessionInfo {
            public_key: [0x11; 32],
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(LEGACY_ACCOUNT),
            identity_chat_private_key: None,
            device_enc_public_key: None,
            lite_username: None,
            full_username: None,
        };

        let resolved = futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], session,
        ));

        // `isPopIssued` denies the undotted claim; `AccountNames` holds the
        // dotted name and `LiteLabelOwner` confirms it.
        assert_eq!(resolved.lite_username.as_deref(), Some("legacy.01"));
        assert_eq!(resolved.full_username, None);
    }
}
