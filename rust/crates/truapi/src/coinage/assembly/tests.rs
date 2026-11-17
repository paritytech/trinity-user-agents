use std::sync::Arc;

use futures::executor::block_on;
use subxt::ext::frame_decode::extrinsics::decode_extrinsic;

use super::*;
use crate::{
	chain::extrinsic_service_serving,
	coinage::{
		PurseId,
		namespace::entry_key,
		testing::{PEOPLE_METADATA_BYTES, chain_params, people_metadata},
	},
	installation::InstallationId,
	runtime::load_vrf,
};

const GENESIS: H256 = H256([0xab; 32]);
const ENTROPY: [u8; 16] = [3; 16];
const INSTALLATION: InstallationId = InstallationId([9; 32]);

fn coin(index: u32) -> Asset {
	Asset::Coin { purse: PurseId::MAIN, installation: INSTALLATION, index }
}

fn entry(index: u32) -> Asset {
	Asset::Entry { purse: PurseId::MAIN, installation: INSTALLATION, index }
}

fn exponent(raw: u8) -> Exponent {
	Exponent::new(raw).unwrap()
}

/// Builds `tx` over a scripted node serving the People fixture, which carries
/// `AsCoinage` in pipeline 0.
fn built(tx: &Transaction, context: &Context, funding: Option<Arc<dyn Funding>>) -> Result<Option<MortalExtrinsic>, AssemblyError> {
	let service = extrinsic_service_serving(PEOPLE_METADATA_BYTES, 0);
	let vrf = block_on(load_vrf()).unwrap();
	let keys = Keys { entropy: &ENTROPY, vrf: &vrf, imported: None, funding, person: None };
	let build = Build { service: &service, genesis: GENESIS, birth: HashAndNumber { hash: H256([4; 32]), number: 4 }, period: 64 };
	block_on(assemble(&build, tx, context, &keys, &chain_params()))
}

/// The call's pallet, name and argument bytes, and `AsCoinage`'s value.
fn decoded(extrinsic: &MortalExtrinsic) -> (String, String, Vec<u8>, Vec<u8>) {
	let metadata = people_metadata();
	let bytes = extrinsic.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let origin = extensions.iter().find(|extension| extension.name() == "AsCoinage").unwrap();
	(
		decoded.pallet_name().to_owned(),
		decoded.call_name().to_owned(),
		bytes[decoded.call_data_args_range()].to_vec(),
		bytes[origin.range()].to_vec(),
	)
}

#[test]
fn a_transfer_is_signed_by_the_coin_and_moves_it_to_the_output_account() {
	let tx = Transaction { call: Call::Transfer, inputs: vec![(coin(0), exponent(2))], outputs: vec![(coin(1), exponent(2))], token: None };
	let extrinsic = built(&tx, &Context::default(), None).unwrap().unwrap();
	let (pallet, call, args, origin) = decoded(&extrinsic);
	let (_, destination) = coin_key(&ENTROPY, PurseId::MAIN, INSTALLATION, 1).unwrap();
	assert_eq!((pallet.as_str(), call.as_str()), ("Coinage", "transfer"));
	assert_eq!(args, destination);
	assert_eq!(origin, [1, 0], "Some(AsCoin)");
	assert_eq!(extrinsic.mortality.period(), 64);
}

#[test]
fn a_split_names_its_outputs_by_ascending_exponent() {
	let tx = Transaction {
		call: Call::Split,
		inputs: vec![(coin(0), exponent(3))],
		outputs: vec![(coin(1), exponent(2)), (coin(2), exponent(1)), (coin(3), exponent(1))],
		token: None,
	};
	let extrinsic = built(&tx, &Context::default(), None).unwrap().unwrap();
	let (_, call, args, _) = decoded(&extrinsic);
	let account = |index| coin_key(&ENTROPY, PurseId::MAIN, INSTALLATION, index).unwrap().1;
	let expected = vec![(1i8, vec![account(2), account(3)]), (2i8, vec![account(1)])].encode();
	assert_eq!((call.as_str(), args), ("split", expected));
}

#[test]
fn a_recycle_loads_the_entry_key_and_proves_the_coin_owns_it() {
	let tx = Transaction { call: Call::Recycle, inputs: vec![(coin(0), exponent(4))], outputs: vec![(entry(0), exponent(4))], token: None };
	let extrinsic = built(&tx, &Context::default(), None).unwrap().unwrap();
	let (_, call, args, _) = decoded(&extrinsic);
	let vrf = block_on(load_vrf()).unwrap();
	let (_, member) = entry_key(&ENTROPY, PurseId::MAIN, INSTALLATION, 0, &vrf).unwrap();
	assert_eq!(call, "load_recycler_with_coin");
	assert_eq!(args[..32], member);
	assert_eq!(args.len(), 32 + 64);
}

struct Declining;

impl Funding for Declining {
	fn account(&self) -> [u8; 32] {
		[5; 32]
	}

	fn sign(&self, _: &[u8]) -> Option<[u8; 64]> {
		None
	}
}

#[test]
fn a_load_its_funding_signer_declines_builds_nothing() {
	let tx = Transaction { call: Call::Load([5; 32]), inputs: Vec::new(), outputs: vec![(entry(0), exponent(4))], token: None };
	let context = Context { ring: None, nonce: Some(3) };
	assert!(built(&tx, &context, Some(Arc::new(Declining))).unwrap().is_none());
}

#[test]
fn an_unload_without_its_ring_or_person_is_not_built() {
	let tx = Transaction {
		call: Call::UnloadToCoins,
		inputs: vec![(entry(0), exponent(4))],
		outputs: vec![(coin(0), exponent(4))],
		token: Some(TokenId { period: 1, slot: 0 }),
	};
	assert!(matches!(built(&tx, &Context::default(), None), Err(AssemblyError::Missing(_))));
}

#[test]
fn a_free_token_context_is_the_prefix_the_period_and_the_counter() {
	let context = free_token_context(TokenId { period: 0x0102_0304, slot: 7 });
	assert_eq!(&context[..24], b"pop:polkadot.net/coinftk");
	assert_eq!(context[24..28], [4, 3, 2, 1]);
	assert_eq!(context[28..], [7, 0, 0, 0]);
}

/// An import is signed with the peer coin's secret the ledger kept, and
/// without it nothing is built.
#[test]
fn an_import_is_signed_with_the_peer_secret_and_needs_it() {
	let (secret, account) = coin_key(&[5; 16], PurseId::MAIN, INSTALLATION, 0).unwrap();
	let tx = Transaction {
		call: Call::Transfer,
		inputs: vec![(Asset::PeerCoin(account), exponent(2))],
		outputs: vec![(coin(1), exponent(2))],
		token: None,
	};
	let service = extrinsic_service_serving(PEOPLE_METADATA_BYTES, 0);
	let vrf = block_on(load_vrf()).unwrap();
	let build = Build { service: &service, genesis: GENESIS, birth: HashAndNumber { hash: H256([4; 32]), number: 4 }, period: 64 };
	let with = |imported| {
		let keys = Keys { entropy: &ENTROPY, vrf: &vrf, imported, funding: None, person: None };
		block_on(assemble(&build, &tx, &Context::default(), &keys, &chain_params()))
	};
	let extrinsic = with(Some(&secret)).unwrap().unwrap();
	let (_, call, args, origin) = decoded(&extrinsic);
	let (_, destination) = coin_key(&ENTROPY, PurseId::MAIN, INSTALLATION, 1).unwrap();
	assert_eq!((call.as_str(), args, origin), ("transfer", destination.to_vec(), vec![1, 0]));
	assert_eq!(signer_of(&extrinsic), account, "the peer's key signs");
	assert!(matches!(with(None), Err(AssemblyError::Missing(_))));
}

/// The account `VerifyMultiSignature` names as the signer, its last field.
fn signer_of(extrinsic: &MortalExtrinsic) -> [u8; 32] {
	let metadata = people_metadata();
	let bytes = extrinsic.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let verify = extensions.iter().find(|extension| extension.name() == "VerifyMultiSignature").unwrap();
	let field = &bytes[verify.range()];
	field[field.len() - 32..].try_into().unwrap()
}
