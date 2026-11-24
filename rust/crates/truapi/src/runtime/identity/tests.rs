//! The in-core lookup reads dotNS over legacy JSON-RPC at one best
//! block. This scripts the Asset Hub node end of those reads: storage values
//! for the pallet keys and `ReviveApi_call` outputs for the contract views, so
//! the whole chain from the best block to classified usernames runs
//! without a network.

use super::*;
use crate::{
	Spawner,
	chain_runtime::{RuntimeChainProvider, RuntimeFailure},
	platform::JsonRpcConnection,
	test_support::{
		DOTNS_NAMED_ACCOUNT as ACCOUNT, DOTNS_SQUATTER as SQUATTER, dotns_usernames_frames,
	},
};
use futures::{StreamExt, channel::mpsc, stream::BoxStream};
use std::sync::{Arc, Mutex};

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
		dotns_usernames_frames(request)
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
fn in_core_lookup_resolves_usernames_at_the_best_block() {
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
		crate::test_support::recorded_rpc_method_count(&platform.sent_rpc, "chain_getBlockHash")
	};

	assert_eq!(reads_for(2), reads_for(1));
}
