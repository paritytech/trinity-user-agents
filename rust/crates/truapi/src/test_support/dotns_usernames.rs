//! An Asset Hub with the dotNS contracts deployed, answering the legacy
//! JSON-RPC reads of a username lookup: storage values for the pallet keys and
//! `ReviveApi_call` outputs for the contract views.

use alloy_primitives::{Address, U256};
use alloy_sol_types::{SolCall, SolInterface};

use super::{
	RpcResponder,
	legacy_asset_hub::{legacy_asset_hub_frames, view_output},
};
use crate::host_logic::dotns_gateway::{
	DotnsPopController, LabelStore, ProtocolRegistry, RootGatewayDispatcher, StoreFactory,
	account_to_h160, dispatcher_address_key,
};

const DISPATCHER: [u8; 20] = [0xd1; 20];
const CONTROLLER: [u8; 20] = [0xc0; 20];
const REGISTRY: [u8; 20] = [0x9e; 20];
const FACTORY: [u8; 20] = [0xfa; 20];
const STORE: [u8; 20] = [0x57; 20];
/// Holds the lite username `alice.01` and the full username `myproject`.
pub const DOTNS_NAMED_ACCOUNT: [u8; 32] = [0xaa; 32];
/// Holds only a publicly registered label the controller never issued.
pub const DOTNS_SQUATTER: [u8; 32] = [0x5a; 32];
const SQUATTER_STORE: [u8; 20] = [0x5b; 20];

/// The scripted account whose H160 a view's `user` argument carries.
fn queried_account(user: Address) -> [u8; 32] {
	[DOTNS_NAMED_ACCOUNT, DOTNS_SQUATTER]
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
fn username_view(dest: &[u8; 20], input: &[u8]) -> Vec<u8> {
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
				let claims = if queried_account(page.user) == DOTNS_NAMED_ACCOUNT {
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
			let store = if queried_account(asked.user) == DOTNS_NAMED_ACCOUNT {
				STORE
			} else {
				SQUATTER_STORE
			};
			StoreFactory::getLabelStoreCall::abi_encode_returns(&store.into())
		},
		STORE => labels_page(input, &["myproject.paseo", "app.myproject.paseo"]),
		SQUATTER_STORE => labels_page(input, &["squatter.paseo"]),
		dest => panic!("unscripted view {} on 0x{}", hex::encode(input), hex::encode(dest)),
	};
	view_output(data, reverted)
}

/// Answers one legacy request of a username lookup, as
/// [`legacy_asset_hub_frames`] does, for the accounts scripted here.
pub fn dotns_usernames_frames(request: &str) -> Vec<String> {
	legacy_asset_hub_frames(
		request,
		|key| (key == dispatcher_address_key()).then(|| DISPATCHER.to_vec()),
		username_view,
	)
}

/// A responder for a platform's chain connections that answers the reads of a
/// username lookup as [`dotns_usernames_frames`] does, and leaves every other
/// request unanswered, such as the ones a session's own channels send.
pub fn dotns_usernames_responder() -> RpcResponder {
	std::sync::Arc::new(|request| {
		let parsed: serde_json::Value = serde_json::from_str(request).expect("a JSON-RPC request");
		let lookup_read = match parsed["method"].as_str() {
			Some("chain_getBlockHash" | "state_getStorage") => true,
			Some("state_call") => parsed["params"][0] == "ReviveApi_call",
			_ => false,
		};
		if lookup_read { dotns_usernames_frames(request) } else { Vec::new() }
	})
}
