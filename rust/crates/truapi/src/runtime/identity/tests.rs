//! The in-core lookup reads dotNS over legacy JSON-RPC at one finalized
//! block. This scripts the Asset Hub node end of those reads: storage values
//! for the pallet keys and `ReviveApi_call` outputs for the contract views, so
//! the whole chain from the finalized head to classified usernames runs
//! without a network.

use super::*;
use crate::{
	Spawner,
	chain_runtime::{RuntimeChainProvider, RuntimeFailure},
	host_logic::dotns_gateway::{
		DotnsPopController, LabelStore, ProtocolRegistry, RootGatewayDispatcher, StoreFactory,
		account_to_h160, dispatcher_address_key,
	},
	platform::JsonRpcConnection,
};
use alloy_primitives::{Address, U256};
use alloy_sol_types::{SolCall, SolInterface};
use futures::{StreamExt, channel::mpsc, stream::BoxStream};
use std::sync::{Arc, Mutex};

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

/// The scripted contract side: `(dest, view)` → the view's whole output.
fn view_output(dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
	use DotnsPopController::DotnsPopControllerCalls as ControllerCall;
	use ProtocolRegistry::ProtocolRegistryCalls as RegistryCall;

	// Discovery probes `protocolRegistry()` first and the dispatcher reverts it; that
	// revert is how the two contracts are told apart, so the mock reproduces it.
	let reverted =
		*dest == DISPATCHER && DotnsPopController::protocolRegistryCall::abi_decode(input).is_ok();
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
			_ => panic!("unscripted controller view {}", hex::encode(input)),
		},
		REGISTRY => match RegistryCall::abi_decode(input).expect("registry view") {
			RegistryCall::get(_) => ProtocolRegistry::getCall::abi_encode_returns(&FACTORY.into()),
			RegistryCall::tld(_) => {
				ProtocolRegistry::tldCall::abi_encode_returns(&".paseo".to_string())
			},
		},
		FACTORY => {
			let asked = StoreFactory::getLabelStoreCall::abi_decode(input).expect("getLabelStore");
			let store = if queried_account(asked.user) == ACCOUNT { STORE } else { SQUATTER_STORE };
			StoreFactory::getLabelStoreCall::abi_encode_returns(&store.into())
		},
		STORE => labels_page(input, &["myproject.paseo", "app.myproject.paseo"]),
		SQUATTER_STORE => labels_page(input, &["squatter.paseo"]),
		dest => panic!("unscripted view {} on 0x{}", hex::encode(input), hex::encode(dest)),
	};
	crate::test_support::view_output(data, reverted)
}

struct ScriptedAssetHub {
	sent: Arc<Mutex<Vec<String>>>,
	sender: Arc<Mutex<Option<mpsc::UnboundedSender<String>>>>,
	receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<String>>>>,
}

impl ScriptedAssetHub {
	fn new() -> Self {
		let (sender, receiver) = mpsc::unbounded();
		Self {
			sent: Arc::new(Mutex::new(Vec::new())),
			sender: Arc::new(Mutex::new(Some(sender))),
			receiver: Arc::new(Mutex::new(Some(receiver))),
		}
	}

	fn share(&self) -> Self {
		Self {
			sent: self.sent.clone(),
			sender: self.sender.clone(),
			receiver: self.receiver.clone(),
		}
	}

	fn frames(&self, request: &str) -> Vec<String> {
		crate::test_support::legacy_asset_hub_frames(
			request,
			|key| (key == dispatcher_address_key()).then(|| DISPATCHER.to_vec()),
			view_output,
		)
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

#[async_trait::async_trait]
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

fn session_without_usernames(public_key: [u8; 32], identity_account_id: [u8; 32]) -> SessionInfo {
	SessionInfo {
		public_key,
		identity_account_id: Some(identity_account_id),
		lite_username: None,
		full_username: None,
		..crate::test_support::session_info()
	}
}

#[test]
fn in_core_lookup_resolves_usernames_at_the_finalized_head() {
	let provider = Arc::new(ScriptedAssetHub::new());
	let chain = ChainRuntime::new(provider.clone(), Spawner::for_test());
	let session = session_without_usernames([0x11; 32], ACCOUNT);

	let resolved = futures::executor::block_on(resolve_session_identity_with_chain(
		&DotnsReads::new(Spawner::for_test()),
		&chain,
		[0xcc; 32],
		session,
	));

	// The pending claim yields the lite name; the settled store yields the
	// full name with the network TLD stripped and the subname dropped.
	assert_eq!(resolved.lite_username.as_deref(), Some("alice.01"));
	assert_eq!(resolved.full_username.as_deref(), Some("myproject"));
	let calls = provider
		.sent
		.lock()
		.unwrap()
		.iter()
		.filter(|request| request.contains("\"state_call\""))
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
	let chain = ChainRuntime::new(provider, Spawner::for_test());
	let session = session_without_usernames(ACCOUNT, SQUATTER);

	let resolved = futures::executor::block_on(resolve_session_identity_with_chain(
		&DotnsReads::new(Spawner::for_test()),
		&chain,
		[0xcc; 32],
		session,
	));

	assert_eq!(
		(resolved.lite_username.as_deref(), resolved.full_username.as_deref()),
		(Some("alice.01"), Some("myproject"))
	);
}

/// Two lookups of one session that overlap read dotNS once between them.
#[test]
fn overlapping_lookups_of_one_session_read_dotns_once() {
	let reads_for = |lookups: usize| {
		let platform = Arc::new(crate::test_support::StubPlatform::default());
		let services = crate::runtime::services::RuntimeServices::new(
			platform.clone(),
			crate::platform::HostInfo {
				name: "Polkadot Mobile".to_string(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			crate::test_support::paseo_test_stack(),
			crate::Spawner::for_test(),
		);
		let session = session_without_usernames([0x11; 32], ACCOUNT);
		futures::executor::block_on(futures::future::join_all((0..lookups).map(|_| {
			lookup_session_identity(&services.dotns_reads, &services.chain, [0xcc; 32], &session)
		})));
		crate::test_support::recorded_rpc_method_count(&platform.sent_rpc, "chain_getFinalizedHead")
	};

	assert_eq!(reads_for(2), reads_for(1));
}
