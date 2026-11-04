//! dotNS identity lookup used to resolve usernames for a session.
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

use crate::{
	chain_runtime::ChainRuntime,
	host_logic::{
		dotns_gateway::{
			DotnsError, DotnsIdentity, classify_labels, discover_pop_controller, resolve_labels,
		},
		session::SessionInfo,
	},
	runtime::dotns_lookup::{DotnsLookup, DotnsLookupKind},
	session_usernames::SessionUsernames,
};

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
	session: SessionInfo,
) -> SessionInfo {
	if session.has_username() {
		return session;
	}
	match lookup_session_identity(chain, asset_hub_chain_genesis_hash, &session).await {
		Some(identity) => {
			let (lite_username, full_username) = identity.into_usernames();
			SessionInfo { lite_username, full_username, ..session }
		},
		None => session,
	}
}

/// Reads `session`'s usernames from the dotNS contracts on Asset Hub, whatever
/// usernames it already carries. The identity account is tried first, then the
/// root key when the identity account holds no username. Returns `None` when no
/// Asset Hub is configured, neither account holds a username, or the lookup
/// fails.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup_session"))]
pub async fn lookup_session_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	session: &SessionInfo,
) -> Option<DotnsIdentity> {
	if asset_hub_chain_genesis_hash == [0; 32] {
		return None;
	}

	let budget = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
	pin_mut!(budget);
	let lookup = async {
		let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
		match lookup_with_retries(
			chain,
			asset_hub_chain_genesis_hash,
			preferred_account,
			"identity",
		)
		.await
		{
			LookupOutcome::Found(identity) => Some(identity),
			LookupOutcome::NoRecord if preferred_account != session.public_key => {
				match lookup_with_retries(
					chain,
					asset_hub_chain_genesis_hash,
					session.public_key,
					"root identity",
				)
				.await
				{
					LookupOutcome::Found(identity) => Some(identity),
					LookupOutcome::NoRecord | LookupOutcome::Failed => None,
				}
			},
			LookupOutcome::NoRecord | LookupOutcome::Failed => None,
		}
	}
	.fuse();
	pin_mut!(lookup);
	futures::select! {
		identity = lookup => identity,
		() = budget => {
			warn!(
				"dotNS username resolution ran out of budget; the session installs without one"
			);
			None
		}
	}
}

/// Maximum lookup attempts per account on transient failure. The first attempt
/// warms the Asset Hub connection, cached per genesis. A retry after a cold-start
/// timeout therefore usually resolves immediately.
const IDENTITY_LOOKUP_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LookupOutcome {
	/// A username record was found.
	Found(DotnsIdentity),
	/// The account has no dotNS username. Definitive, not worth a retry.
	NoRecord,
	/// The lookup failed transiently after exhausting retries.
	Failed,
}

/// Looks up `account`'s dotNS identity, retrying transient failures against the
/// warmed connection.
async fn lookup_with_retries(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account: [u8; 32],
	label: &str,
) -> LookupOutcome {
	for attempt in 1..=IDENTITY_LOOKUP_MAX_ATTEMPTS {
		match lookup_dotns_identity(chain, asset_hub_chain_genesis_hash, account).await {
			Ok(Some(identity)) => {
				debug!(
					account = %hex::encode(account),
					lite_username = ?identity.lite_username(),
					full_username = ?identity.full_username(),
					"dotNS {label} lookup found username"
				);
				return LookupOutcome::Found(identity);
			},
			Ok(None) => {
				debug!(
					account = %hex::encode(account),
					"dotNS {label} lookup found no username"
				);
				return LookupOutcome::NoRecord;
			},
			Err(reason) => {
				// A warning leaves the device, so the account is named by a
				// handle rather than in full. The debug lines above, which stay
				// on it, still carry the whole id.
				warn!(
					account = %crate::logging::account_handle(&account),
					attempt,
					reason = %reason.reason(),
					"dotNS {label} lookup failed"
				);
			},
		}
	}
	LookupOutcome::Failed
}

/// Resolves `account_id`'s usernames from the dotNS contracts at a fresh Asset
/// Hub head. Each step carries the lookup transport's own step timeout; the caller's
/// [`LOOKUP_BUDGET`] bounds the whole resolution. Returns `None` when the
/// gateway is not deployed. Also returns `None` when none of the account's
/// labels is a username.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_dotns_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account_id: [u8; 32],
) -> Result<Option<DotnsIdentity>, DotnsError> {
	let lookup = async {
		let mut lookup = DotnsLookup::pinned_to_best_block(
			chain,
			asset_hub_chain_genesis_hash,
			DotnsLookupKind::Identity,
		)
		.await?;
		let Some(controller) = discover_pop_controller(&mut lookup).await? else {
			return Ok(None);
		};
		let labels = resolve_labels(&mut lookup, &controller, &account_id).await?;
		classify_labels(&mut lookup, &controller, labels).await
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
	use crate::{
		chain::{ContractResult, ExecReturnValue, StorageDeposit, Weight},
		chain_runtime::{RuntimeChainProvider, RuntimeFailure},
		host_logic::dotns_gateway::{
			DotnsPopController, LabelStore, ProtocolRegistry, RootGatewayDispatcher, StoreFactory,
			account_to_h160, dispatcher_address_key,
		},
		platform::JsonRpcConnection,
		subscription::thread_per_subscription_spawner,
	};
	use alloy_primitives::{Address, U256};
	use alloy_sol_types::{SolCall, SolInterface};
	use async_trait::async_trait;
	use futures::{StreamExt, channel::mpsc, stream::BoxStream};
	use parity_scale_codec::{Decode, Encode};
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
	/// Holds only a publicly registered label the controller never issued.
	const SQUATTER: [u8; 32] = [0x5a; 32];
	const SQUATTER_STORE: [u8; 20] = [0x5b; 20];

	/// The scripted account whose H160 a view's `user` argument carries.
	fn queried_account(user: Address) -> [u8; 32] {
		[ACCOUNT, SQUATTER]
			.into_iter()
			.find(|account| user == Address::from(account_to_h160(account)))
			.expect("view for an unscripted account")
	}

	/// A `LabelStore.getLabels` answer of `labels`, read as the first page.
	fn labels_page(input: &[u8], labels: &[&str]) -> Vec<u8> {
		let page = LabelStore::getLabelsCall::abi_decode(input).expect("getLabels");
		assert_eq!((page.offset, page.limit), (U256::ZERO, U256::from(16)));
		LabelStore::getLabelsCall::abi_encode_returns(
			&labels.iter().map(|label| label.to_string()).collect(),
		)
	}

	/// The scripted contract side: `(dest, view)` → return data.
	fn view_output(dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
		use DotnsPopController::DotnsPopControllerCalls as ControllerCall;
		use ProtocolRegistry::ProtocolRegistryCalls as RegistryCall;

		// Discovery probes `protocolRegistry()` first and the dispatcher reverts it; that
		// revert is how the two contracts are told apart, so the mock reproduces it.
		let reverted = *dest == DISPATCHER &&
			DotnsPopController::protocolRegistryCall::abi_decode(input).is_ok();
		let data = match *dest {
			_ if reverted => Vec::new(),
			DISPATCHER => {
				RootGatewayDispatcher::TARGETCall::abi_decode(input).expect("TARGET()");
				RootGatewayDispatcher::TARGETCall::abi_encode_returns(&CONTROLLER.into())
			},
			CONTROLLER => match ControllerCall::abi_decode(input).expect("controller view") {
				ControllerCall::pendingClaims(page) => {
					assert_eq!((page.offset, page.limit), (U256::ZERO, U256::from(16)));
					// Minted at the epoch: a pending claim never lapses.
					let claims = if queried_account(page.user) == ACCOUNT {
						vec![DotnsPopController::PendingClaim {
							label: "alice.01".to_string(),
							mintedAt: 1,
						}]
					} else {
						Vec::new()
					};
					DotnsPopController::pendingClaimsCall::abi_encode_returns(&claims)
				},
				ControllerCall::isPopIssued(asked) => {
					DotnsPopController::isPopIssuedCall::abi_encode_returns(
						&(asked.label != "squatter"),
					)
				},
				ControllerCall::protocolRegistry(_) => {
					DotnsPopController::protocolRegistryCall::abi_encode_returns(&REGISTRY.into())
				},
			},
			REGISTRY => match RegistryCall::abi_decode(input).expect("registry view") {
				RegistryCall::get(_) => {
					ProtocolRegistry::getCall::abi_encode_returns(&FACTORY.into())
				},
				RegistryCall::tld(_) => {
					ProtocolRegistry::tldCall::abi_encode_returns(&".paseo".to_string())
				},
			},
			FACTORY => {
				let asked =
					StoreFactory::getLabelStoreCall::abi_decode(input).expect("getLabelStore");
				let store =
					if queried_account(asked.user) == ACCOUNT { STORE } else { SQUATTER_STORE };
				StoreFactory::getLabelStoreCall::abi_encode_returns(&store.into())
			},
			STORE => labels_page(input, &["myproject.paseo", "app.myproject.paseo"]),
			SQUATTER_STORE => labels_page(input, &["squatter.paseo"]),
			dest => panic!("unscripted view {} on 0x{}", hex::encode(input), hex::encode(dest)),
		};
		ContractResult {
			_weight_consumed: Weight::default(),
			weight_required: Weight::default(),
			storage_deposit: StorageDeposit::Charge(0),
			max_storage_deposit: StorageDeposit::Charge(0),
			_gas_consumed: 0,
			result: Ok(ExecReturnValue { flags: u32::from(reverted), data }),
		}
		.encode()
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
				},
				"chainHead_v1_storage" => {
					*next_operation += 1;
					let operation_id = format!("storage-{}", *next_operation);
					assert_eq!(
						[&request["params"][0], &request["params"][1]],
						[FOLLOW_ID, BEST_HASH]
					);
					let key = request["params"][2][0]["key"].as_str().unwrap();
					let key_bytes = hex::decode(key.trim_start_matches("0x")).unwrap();
					let mut frames =
						vec![response(json!({"result": "started", "operationId": operation_id}))];
					if key_bytes == dispatcher_address_key() {
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
				},
				"chainHead_v1_call" => {
					*next_operation += 1;
					let operation_id = format!("call-{}", *next_operation);
					assert_eq!(
						[&request["params"][0], &request["params"][1]],
						[FOLLOW_ID, BEST_HASH]
					);
					assert_eq!(request["params"][2].as_str(), Some("ReviveApi_call"));
					let args = hex::decode(
						request["params"][3].as_str().unwrap().trim_start_matches("0x"),
					)
					.unwrap();
					// origin[32] ‖ dest[20] ‖ value u128 ‖ None ‖ None ‖ Vec(input).
					assert_eq!(&args[..32], &[0xEE; 32]);
					assert_eq!(&args[52..70], &[0u8; 18], "zero value, no gas or deposit limit");
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
				},
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
			self.receiver.lock().unwrap().take().expect("responses called once").boxed()
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

	fn session_without_usernames(
		public_key: [u8; 32],
		identity_account_id: [u8; 32],
	) -> SessionInfo {
		SessionInfo {
			public_key,
			identity_account_id: Some(identity_account_id),
			lite_username: None,
			full_username: None,
			..crate::test_support::session_info()
		}
	}

	#[test]
	fn in_core_lookup_resolves_usernames_over_one_runtime_follow() {
		let provider = Arc::new(ScriptedAssetHub::new());
		let chain = ChainRuntime::new(provider.clone(), thread_per_subscription_spawner());
		let session = session_without_usernames([0x11; 32], ACCOUNT);

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
		// getLabels page, then one isPopIssued per label (alice.01, myproject).
		// A repointed chain resolves on the first probe and needs nine.
		assert_eq!(
			calls, 10,
			"the discovery, label and provenance chain is exactly ten views on a dispatcher chain"
		);
	}

	/// Labels the controller did not issue are no username, so an identity
	/// account holding only those must not stop the root-key fallback.
	#[test]
	fn root_key_resolves_when_identity_account_labels_are_unissued() {
		let provider = Arc::new(ScriptedAssetHub::new());
		let chain = ChainRuntime::new(provider, thread_per_subscription_spawner());
		let session = session_without_usernames(ACCOUNT, SQUATTER);

		let resolved = futures::executor::block_on(resolve_session_identity_with_chain(
			&chain, [0xcc; 32], session,
		));

		assert_eq!(
			(resolved.lite_username.as_deref(), resolved.full_username.as_deref()),
			(Some("alice.01"), Some("myproject"))
		);
	}
}
