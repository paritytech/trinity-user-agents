//! dotNS identity lookup used to resolve usernames for a paired session.
//!
//! Usernames live in the dotNS contracts on Asset Hub. The gateway pallet
//! storage anchors the `DotnsPopController`. The protocol registry locates the
//! `StoreFactory`. The account's labels come from its `LabelStore` on the warm
//! path. On the cold path they come from its pending claim on the controller,
//! covering gateway-minted names before the user settles their store.
//!
//! All reads run over one `chainHead_v1` follow via `ReviveApi_call` dry-runs.
//! No chain metadata is needed.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::chain_runtime::ChainRuntime;
use crate::host_logic::dotns_gateway::{
    DotnsIdentity, DotnsTransport, classify_labels, discover_pop_controller, resolve_labels,
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
/// Cached names remain attached to the supplied session. A successful lookup
/// without names is distinct from unavailable discovery, transport, or budget.
/// Callers installing a session may retain it on failure; disclosure callers
/// must propagate the failure rather than report a confirmed missing username.
#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
pub async fn resolve_session_identity_with_chain(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    session: &mut SessionInfo,
) -> Result<(), String> {
    if session.has_username() {
        return Ok(());
    }
    if asset_hub_chain_genesis_hash == [0; 32] {
        return Err("dotNS username lookup unavailable: no Asset Hub configured".to_string());
    }

    let budget = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
    pin_mut!(budget);
    let resolve = async {
        let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
        if lookup_and_apply(
            chain,
            asset_hub_chain_genesis_hash,
            preferred_account,
            session,
            "identity",
        )
        .await?
            == LookupOutcome::NoRecord
            && preferred_account != session.public_key
        {
            let public_key = session.public_key;
            lookup_and_apply(
                chain,
                asset_hub_chain_genesis_hash,
                public_key,
                session,
                "root identity",
            )
            .await?;
        }
        Ok(())
    }
    .fuse();
    pin_mut!(resolve);
    futures::select! {
        result = resolve => result,
        () = budget => {
            Err("dotNS username lookup unavailable: resolution exceeded 45-second budget".to_string())
        }
    }
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
}

enum IdentityLookupError {
    /// Discovery succeeded, but the required gateway is not deployed.
    Unavailable(&'static str),
    /// Preserve the existing bounded retries for failed chain reads.
    Failed(String),
}

impl From<String> for IdentityLookupError {
    fn from(reason: String) -> Self {
        Self::Failed(reason)
    }
}

/// Looks up `account`'s dotNS identity and applies any usernames to `session`.
/// Transient failures are retried against the warmed connection.
async fn lookup_and_apply(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    account: [u8; 32],
    session: &mut SessionInfo,
    label: &str,
) -> Result<LookupOutcome, String> {
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
                return Ok(LookupOutcome::Applied);
            }
            Ok(None) => {
                debug!(
                    account = %hex::encode(account),
                    "dotNS {label} lookup found no labels"
                );
                return Ok(LookupOutcome::NoRecord);
            }
            Err(IdentityLookupError::Unavailable(reason)) => {
                return Err(format!("dotNS {label} username lookup unavailable: {reason}"));
            }
            Err(IdentityLookupError::Failed(reason)) => {
                warn!(
                    account = %hex::encode(account),
                    attempt,
                    %reason,
                    "dotNS {label} lookup failed"
                );
                if attempt == IDENTITY_LOOKUP_MAX_ATTEMPTS {
                    return Err(format!(
                        "dotNS {label} username lookup unavailable after {attempt} attempts: {reason}"
                    ));
                }
            }
        }
    }
    unreachable!("identity lookup always attempts at least once")
}

/// Resolves `account_id`'s usernames from the dotNS contracts at a fresh Asset
/// Hub head. Each step carries the lookup transport's own step timeout; the caller's
/// [`LOOKUP_BUDGET`] bounds the whole resolution. Returns `None` only when
/// the account holds no labels; absent gateway discovery is unavailable.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_dotns_identity(
    chain: &ChainRuntime,
    asset_hub_chain_genesis_hash: [u8; 32],
    account_id: [u8; 32],
) -> Result<Option<DotnsIdentity>, IdentityLookupError> {
    let lookup = async {
        let mut lookup = DotnsLookup::pinned_to_best_block(
            chain,
            asset_hub_chain_genesis_hash,
            &format!("identity:{}", hex::encode(account_id)),
        )
        .await?;
        let Some(controller) = discover_pop_controller(&mut lookup).await? else {
            return Err(IdentityLookupError::Unavailable(
                "dotNS gateway is not deployed on the configured Asset Hub",
            ));
        };
        let labels = resolve_labels(&mut lookup, &controller, &account_id).await?;
        if labels.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            classify_labels(&mut lookup, &controller, &account_id, labels).await?,
        ))
    }
    .fuse();
    pin_mut!(lookup);
    lookup.await
}

/// Resolve the account's Lite identity, requiring gateway ownership as well as PoP provenance.
pub(super) async fn lookup_local_identity(
    chain: &ChainRuntime,
    genesis: [u8; 32],
    account: [u8; 32],
) -> Result<DotnsIdentity, String> {
    let operation = async {
        let mut lookup = DotnsLookup::pinned_to_best_block(
            chain,
            genesis,
            &format!("local-identity:{}", hex::encode(account)),
        )
        .await?;
        let controller = discover_pop_controller(&mut lookup)
            .await?
            .ok_or("dotNS gateway is not deployed on the configured Asset Hub")?;
        let labels = resolve_labels(&mut lookup, &controller, &account).await?;
        let mut owned = Vec::new();
        for label in labels {
            if !crate::host_logic::dotns_gateway::is_dotted_lite_username(&label) {
                continue;
            }
            let owner = lookup
                .storage(crate::host_logic::dotns_gateway::lite_label_owner_key(
                    label.as_bytes(),
                ))
                .await?;
            let Some(owner) = owner else { continue };
            let owner: [u8; 32] = owner
                .as_slice()
                .try_into()
                .map_err(|_| "DotnsGateway.LiteLabelOwner is not a 32-byte account")?;
            if owner == account {
                owned.push(label);
            }
        }
        classify_labels(&mut lookup, &controller, &account, owned).await
    }
    .fuse();
    let timeout = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
    pin_mut!(operation, timeout);
    futures::select! {
        result = operation => result,
        () = timeout => Err("dotNS identity lookup timed out".to_string()),
    }
}

/// Read the reservation timestamp from Asset Hub rather than the browser clock.
pub(super) async fn registration_timestamp(
    chain: &ChainRuntime,
    genesis: [u8; 32],
    account: [u8; 32],
) -> Result<u64, String> {
    let mut lookup = DotnsLookup::pinned_to_best_block(
        chain,
        genesis,
        &format!("registration:{}", hex::encode(account)),
    )
    .await?;
    let value = lookup
        .storage(crate::host_logic::dotns_gateway::timestamp_now_key())
        .await?
        .ok_or("Timestamp.Now is unset")?;
    let millis: [u8; 8] = value
        .as_slice()
        .try_into()
        .map_err(|_| "Timestamp.Now is not a u64")?;
    Ok(u64::from_le_bytes(millis) / 1000)
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
        VIEW_CALL_ORIGIN, account_to_h160, dispatcher_address_key, selector,
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FOLLOW_ID: &str = "ah-follow";
    const BEST_HASH: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
    const DISPATCHER: [u8; 20] = [0xd1; 20];
    const CONTROLLER: [u8; 20] = [0xc0; 20];
    const REGISTRY: [u8; 20] = [0x9e; 20];
    const NAME_REGISTRY: [u8; 20] = [0x9f; 20];
    const FACTORY: [u8; 20] = [0xfa; 20];
    const STORE: [u8; 20] = [0x57; 20];
    const ACCOUNT: [u8; 32] = [0xaa; 32];

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
                // First page for the mapped identity account.
                assert_eq!(
                    &input[4..36],
                    abi_address(&account_to_h160(&ACCOUNT)).as_slice()
                );
                assert_eq!(&input[36..68], &abi_word(0));
                assert_eq!(&input[68..100], &abi_word(16));
                // A transferred candidate must not win; the still-owned name
                // was minted long before the old seven-day reservation cutoff.
                abi_pending_claims(&[
                    ("transferred.01", 1_800_000_000),
                    ("alice.01", 1),
                ])
            }
            (CONTROLLER, s) if s == selector("isPopIssued(string)") => {
                // Provenance alone cannot authorize the transferred name.
                abi_word(1).to_vec()
            }
            (CONTROLLER, s) if s == selector("protocolRegistry()") => abi_address(&REGISTRY),
            (REGISTRY, s) if s == selector("get(bytes32)") => {
                if input[4..] == crate::host_logic::dotns_gateway::registry_key("registry") {
                    abi_address(&NAME_REGISTRY)
                } else {
                    abi_address(&FACTORY)
                }
            }
            (REGISTRY, s) if s == selector("tld()") => abi_string(".paseo"),
            (NAME_REGISTRY, s) if s == selector("recordExists(bytes32)") => {
                let tld = crate::dotns_views::tld_node(".paseo");
                let present = ["transferred.01", "alice.01", "myproject"].iter().any(|label| {
                    input[4..] == crate::host_logic::dotns_gateway::namehash_under(&tld, label)
                });
                abi_word(u64::from(present)).to_vec()
            }
            (NAME_REGISTRY, s) if s == selector("owner(bytes32)") => {
                let transferred = crate::host_logic::dotns_gateway::namehash_under(
                    &crate::dotns_views::tld_node(".paseo"), "transferred.01",
                );
                if input[4..] == transferred {
                    abi_address(&[0xbb; 20])
                } else {
                    abi_address(&account_to_h160(&ACCOUNT))
                }
            }
            (FACTORY, s) if s == selector("getLabelStore(address)") => {
                assert_eq!(&input[16..36], account_to_h160(&ACCOUNT));
                abi_address(&STORE)
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

    #[derive(Clone, Copy, Default)]
    enum ScriptedLookup {
        #[default]
        Labels,
        NoRecord,
        Unavailable,
        GatewayMissing,
    }

    struct ScriptedAssetHub {
        sent: Arc<Mutex<Vec<String>>>,
        follow_with_runtime: Arc<Mutex<Option<bool>>>,
        sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
        receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
        next_operation: Arc<Mutex<u64>>,
        lookup: ScriptedLookup,
        lookup_requests: Arc<AtomicUsize>,
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
                lookup: ScriptedLookup::Labels,
                lookup_requests: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn share(&self) -> Self {
            Self {
                sent: self.sent.clone(),
                follow_with_runtime: self.follow_with_runtime.clone(),
                sender: self.sender.clone(),
                receiver: self.receiver.clone(),
                next_operation: self.next_operation.clone(),
                lookup: self.lookup,
                lookup_requests: self.lookup_requests.clone(),
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
                    if key_bytes == dispatcher_address_key()
                        && !matches!(self.lookup, ScriptedLookup::GatewayMissing)
                    {
                        frames.push(follow_event(json!({
                            "event": "operationStorageItems",
                            "operationId": operation_id,
                            "items": [{"key": key, "value": format!("0x{}", hex::encode(DISPATCHER))}]
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
                    let sel: [u8; 4] = input[..4].try_into().unwrap();
                    if dest == FACTORY && sel == selector("getLabelStore(address)") {
                        assert_eq!(&input[16..36], account_to_h160(&ACCOUNT));
                        self.lookup_requests.fetch_add(1, Ordering::SeqCst);
                        if matches!(self.lookup, ScriptedLookup::Unavailable) {
                            return vec![json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": {"code": -32000, "message": "identity lookup offline"}
                            }).to_string()];
                        }
                    }
                    let output = match self.lookup {
                        ScriptedLookup::NoRecord
                            if dest == CONTROLLER
                                && sel == selector("pendingClaims(address,uint256,uint256)") =>
                        {
                            contract_result(&abi_pending_claims(&[]))
                        }
                        ScriptedLookup::NoRecord
                            if dest == FACTORY && sel == selector("getLabelStore(address)") =>
                        {
                            contract_result(&abi_address(&[0; 20]))
                        }
                        _ => view_output(&dest, &input),
                    };
                    vec![
                        response(json!({"result": "started", "operationId": operation_id})),
                        follow_event(json!({
                            "event": "operationCallDone",
                            "operationId": operation_id,
                            "output": format!("0x{}", hex::encode(output))
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
        let mut resolved = SessionInfo {
            public_key: [0x11; 32],
            sso: None,
            root_entropy_source: None,
            identity_account_id: Some(ACCOUNT),
            identity_chat_private_key: None,
            device_enc_public_key: None,
            lite_username: None,
            full_username: None,
        };

        futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], &mut resolved,
        ))
        .unwrap();

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
        // Discovery and label enumeration use eight views. Classification
        // reads provenance for all three candidates, discovers registry/TLD
        // once, and checks atomic plus nested owners for both dotted names.
        assert_eq!(
            calls, 22,
            "discovery, provenance and current ownership share the runtime follow"
        );
    }

    fn unnamed_session() -> SessionInfo {
        let mut session = crate::test_support::sso_session_info();
        session.identity_account_id = Some(ACCOUNT);
        session.lite_username = None;
        session.full_username = None;
        session
    }

    #[test]
    fn lookup_failure_preserves_owner_and_does_not_try_root_account() {
        let mut provider = ScriptedAssetHub::new();
        provider.lookup = ScriptedLookup::Unavailable;
        let provider = Arc::new(provider);
        let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
        let mut session = unnamed_session();
        let original = session.clone();
        let error = futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], &mut session,
        ))
        .unwrap_err();
        assert!(error.contains("after 3 attempts"), "{error}");
        assert!(error.contains("identity lookup offline"), "{error}");
        assert_eq!(session, original, "partial labels must not change identity or XID");
        assert_eq!(
            provider.lookup_requests.load(Ordering::SeqCst),
            IDENTITY_LOOKUP_MAX_ATTEMPTS,
        );
    }

    #[test]
    fn successful_empty_lookup_is_not_unavailability() {
        let mut provider = ScriptedAssetHub::new();
        provider.lookup = ScriptedLookup::NoRecord;
        let provider = Arc::new(provider);
        let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
        let mut session = unnamed_session();
        session.public_key = ACCOUNT;
        let original = session.clone();
        futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], &mut session,
        ))
        .unwrap();
        assert_eq!(session, original);
        assert_eq!(
            provider.lookup_requests.load(Ordering::SeqCst),
            1,
            "confirmed absence is not retried",
        );
    }

    #[test]
    fn missing_gateway_is_unavailable_not_missing_username() {
        let mut provider = ScriptedAssetHub::new();
        provider.lookup = ScriptedLookup::GatewayMissing;
        let chain = ChainRuntime::new(Arc::new(provider), thread_per_subscription_spawner());
        let mut session = unnamed_session();
        let original = session.clone();
        let error = futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0xcc; 32], &mut session,
        ))
        .unwrap_err();
        assert!(error.contains("gateway is not deployed"), "{error}");
        assert_eq!(session, original);
    }

    #[test]
    fn cached_username_stays_on_its_session_without_chain_lookup() {
        let provider = Arc::new(ScriptedAssetHub::new());
        let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
        let mut session = unnamed_session();
        session.lite_username = Some("alice.01".to_string());
        let original = session.clone();
        futures::executor::block_on(resolve_session_identity_with_chain(
            &chain, [0; 32], &mut session,
        ))
        .unwrap();
        assert_eq!(session, original);
        assert_eq!(provider.lookup_requests.load(Ordering::SeqCst), 0);
    }
}
