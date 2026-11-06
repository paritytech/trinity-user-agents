use super::*;
use crate::dotns_views::{TLD_WITHOUT_VIEW, network_tld};
use alloy_sol_types::SolCall;

// Golden vectors from the validated reference implementation in
// pop-dotns-testing: reservation.ts, proof.ts, verify.ts.
const RESERVATION_WITH_RESERVED: &str = "64706f703a646f746e732d676174657761793a72657365727665111111111111111111111111111111111111111111111111111111111111111122222222222222222222222222222222222222222222222222222222222222221c616c696365626305013333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333013072657365727665646e616d65035e486800000000";
const RESERVATION_NO_RESERVED: &str = "64706f703a646f746e732d676174657761793a72657365727665111111111111111111111111111111111111111111111111111111111111111122222222222222222222222222222222222222222222222222222222222222221c616c69636562630501333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333300035e486800000000";

#[test]
fn reservation_message_matches_reference_vectors() {
	let with_reserved = build_reservation_message(
		&[0x11; 32],
		&[0x22; 32],
		b"alicebc",
		&[0x33; 65],
		Some(b"reservedname"),
		1749573123,
	);
	assert_eq!(hex::encode(&with_reserved), RESERVATION_WITH_RESERVED);

	let without = build_reservation_message(
		&[0x11; 32],
		&[0x22; 32],
		b"alicebc",
		&[0x33; 65],
		None,
		1749573123,
	);
	assert_eq!(hex::encode(&without), RESERVATION_NO_RESERVED);
}

#[test]
fn link_and_proof_message_match_reference_vectors() {
	let lite = Link::LiteUsername(b"alice.01".to_vec());
	assert_eq!(hex::encode(lite.encode()), "0020616c6963652e3031");
	assert_eq!(
		hex::encode(build_register_proof_message(&[0x44; 32], b"alicebc", &lite)),
		"506fe3afcb13b2e4fd182d49ff165bc9c834f9dd21d0d545eccb925993b68675"
	);

	let standalone = Link::None([0x55; 65]);
	assert_eq!(hex::encode(standalone.encode()), format!("01{}", hex::encode([0x55; 65])));
	assert_eq!(
		hex::encode(build_register_proof_message(&[0x44; 32], b"alicebc", &standalone)),
		"255da65ea6687123d5ac035a438a5c6fa18f7d04f23433611095a0fe58c479ab"
	);
}

/// The call arguments encode by name against Asset Hub's metadata into the
/// gateway's positional layout: account, label, link.
#[test]
fn register_name_call_args_encode_to_the_reference_layout() {
	use subxt::{Metadata, ext::frame_decode::extrinsics::encode_call_data};

	let metadata =
		Metadata::decode_from(crate::runtime::statement_allowance::ASSET_HUB_TEST_METADATA_BYTES)
			.unwrap();
	let link = Link::LiteUsername(b"alice.01".to_vec());
	let args =
		RegisterNameCallArgs { who: [0x44; 32], label: b"alicebc".to_vec(), link: link.clone() };

	let call =
		encode_call_data("DotnsGateway", "register_name", &args, &metadata, metadata.types())
			.unwrap();

	let arguments = [vec![0x44; 32], b"alicebc".to_vec().encode(), link.encode()].concat();
	assert_eq!(&call[2..], &arguments[..]);
}

/// The registration value encodes by name into the gateway's positional
/// layout: `Some ‖ RegisterFullName ‖ proof ‖ ring index ‖ revision ‖
/// Sr25519 ‖ signature`.
#[test]
fn a_full_name_registration_encodes_as_the_gateway_expects() {
	use subxt::{Metadata, ext::frame_decode::extrinsics::ExtrinsicTypeInfo};

	let metadata =
		Metadata::decode_from(crate::runtime::statement_allowance::ASSET_HUB_TEST_METADATA_BYTES)
			.unwrap();
	let pipeline = metadata.extrinsic_extension_info(Some(0)).unwrap();
	let slot = pipeline
		.extension_ids
		.iter()
		.find(|declared| declared.name == "AsDotnsGateway")
		.unwrap();
	let info = AsDotnsGatewayInfo::RegisterFullName {
		proof: vec![0xEE; 4],
		ring_index: 3,
		revision: 5,
		signature: MultiSignature::Sr25519([0xAB; 64]),
	};

	let encoded = Some(info).encode_as_type(slot.id, metadata.types()).unwrap();

	let expected = [
		vec![0x01, 0x00],
		vec![0xEEu8; 4].encode(),
		3u32.encode(),
		5u32.encode(),
		vec![0x01],
		vec![0xAB; 64],
	]
	.concat();
	assert_eq!(encoded, expected);
}

#[test]
fn namehash_matches_the_reference_derivation() {
	// `cast namehash paseo` and `cast namehash alicebc.paseo`.
	let tld_node = namehash_under(&[0u8; 32], "paseo");
	assert_eq!(
		hex::encode(tld_node),
		"096b436ee9a398429fe33ad4b359bad4398dd74b412ec1dd043c93dfbf581874"
	);
	assert_eq!(
		hex::encode(namehash_under(&tld_node, "alicebc")),
		"d3b106171373cd9fea783acd85f69def1bb2085f81cf5f2a65088b0ce4edd82e"
	);
	assert_eq!(crate::dotns_views::tld_node(".paseo"), tld_node);
	assert_eq!(crate::dotns_views::tld_node("paseo"), tld_node);
}

#[test]
fn account_to_h160_follows_the_revive_mapper() {
	let mut eth_derived = [0x0Fu8; 32];
	eth_derived[20..].fill(0xEE);
	assert_eq!(account_to_h160(&eth_derived), [0x0F; 20]);

	let hashed = account_to_h160(&[0x11; 32]);
	assert_eq!(hashed, keccak_256(&[0x11; 32])[12..]);
}

/// Answers a view of `Call` with `value`.
fn returns<Call: SolCall>(value: Call::Return) -> Result<Vec<u8>, EvmContractError> {
	Ok(Call::abi_encode_returns(&value))
}

/// The view of `Call` that `input` encodes, if it encodes one.
fn decoded<Call: SolCall>(input: &[u8]) -> Option<Call> {
	Call::abi_decode(input).ok()
}

fn view_reverted() -> Result<Vec<u8>, EvmContractError> {
	Err(EvmContractError::Reverted { detail: "(empty)".to_string() })
}

struct RevertingSecondClaimPage {
	first_page: Vec<DotnsPopController::PendingClaim>,
	pending_calls: usize,
}

#[async_trait::async_trait]
impl DotnsTransport for RevertingSecondClaimPage {
	async fn storage(&mut self, key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		panic!("unscripted storage read {}", hex::encode(key));
	}

	async fn view(
		&mut self,
		_dest: &[u8; 20],
		input: Vec<u8>,
	) -> Result<Vec<u8>, EvmContractError> {
		let page = DotnsPopController::pendingClaimsCall::abi_decode(&input)
			.expect("only pending claims are scripted");
		self.pending_calls += 1;
		assert_eq!(page.limit, U256::from(CLAIM_PAGE_LIMIT));
		if page.offset.is_zero() {
			return returns::<DotnsPopController::pendingClaimsCall>(self.first_page.clone());
		}
		assert_eq!(page.offset, U256::from(CLAIM_PAGE_LIMIT));
		Err(EvmContractError::Reverted { detail: "offset past end".to_string() })
	}
}

#[test]
fn a_later_pending_claim_page_revert_keeps_complete_earlier_pages() {
	let expected = (0..CLAIM_PAGE_LIMIT)
		.map(|index| format!("claim{index:02}"))
		.collect::<Vec<_>>();
	let mut transport = RevertingSecondClaimPage {
		first_page: expected
			.iter()
			.cloned()
			.map(|label| DotnsPopController::PendingClaim { label, mintedAt: 50 })
			.collect(),
		pending_calls: 0,
	};

	let labels =
		futures::executor::block_on(pending_claim_labels(&mut transport, &[0xc0; 20], &[0xaa; 20]))
			.unwrap();

	assert_eq!(labels, expected);
	assert_eq!(transport.pending_calls, 2);
}

#[test]
fn labels_classify_into_lite_and_full_usernames() {
	futures::executor::block_on(async {
		// Every label the transport is asked about counts as pop-issued
		// unless listed; assertions below separate shape from provenance.
		struct IssuedAll {
			denied: Vec<&'static str>,
		}
		#[async_trait::async_trait]
		impl DotnsTransport for IssuedAll {
			async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
				unreachable!("classify_labels reads no storage")
			}
			async fn view(
				&mut self,
				_dest: &[u8; 20],
				input: Vec<u8>,
			) -> Result<Vec<u8>, EvmContractError> {
				let asked = DotnsPopController::isPopIssuedCall::abi_decode(&input)
					.expect("only isPopIssued is scripted");
				returns::<DotnsPopController::isPopIssuedCall>(
					!self.denied.contains(&asked.label.as_str()),
				)
			}
		}
		let identity = |lite: Option<&str>, full: Option<&str>| {
			Some(DotnsIdentity {
				lite_username: lite.map(str::to_owned),
				full_username: full.map(str::to_owned),
			})
		};
		let controller = [0xAA; 20];
		let mut all = IssuedAll { denied: vec![] };

		assert_eq!(
			classify_labels(&mut all, &controller, ["alice.01", "myproject"]).await.unwrap(),
			identity(Some("alice.01"), Some("myproject"))
		);

		// The lite stem is letters only (`isLitePersonLabel`): a digit or
		// hyphen in the stem is not a lite label. The flat spelling is an
		// ordinary label, and dotted non-lite labels are subnames.
		assert_eq!(
			classify_labels(
				&mut all,
				&controller,
				["app.web3app", "sub.alice.01", "aé.01", "a2b.34", "-x.01", "alice01"],
			)
			.await
			.unwrap(),
			identity(None, Some("alice01"))
		);

		assert_eq!(
			classify_labels(&mut all, &controller, Vec::<String>::new()).await.unwrap(),
			None
		);

		// Provenance gates every slot: a perfectly shaped label the
		// controller did not issue is skipped.
		let mut denying = IssuedAll { denied: vec!["app.42", "squatter"] };
		assert_eq!(
			classify_labels(
				&mut denying,
				&controller,
				["app.42", "squatter", "alice.01", "myproject"],
			)
			.await
			.unwrap(),
			identity(Some("alice.01"), Some("myproject"))
		);
		// An account whose labels are all unissued has no identity.
		assert_eq!(
			classify_labels(&mut denying, &controller, ["app.42", "squatter"])
				.await
				.unwrap(),
			None
		);

		// Hostile store data is dropped before the provenance read: the
		// transport panics on storage and asserts the selector, so reaching
		// it with garbage would fail loudly.
		assert_eq!(
			classify_labels(
				&mut all,
				&controller,
				[
					"a".repeat(5000),
					"admin\r\nx".to_string(),
					"a\0b".to_string(),
					"\x1b[31mred\x1b[0m".to_string(),
					"<img src=x onerror=alert(1)>".to_string(),
					"Upper".to_string(),
				],
			)
			.await
			.unwrap(),
			None
		);

		// The 63-octet DNS bound is the cut-off for full labels; the lite
		// dotted form is capped by the 32-byte gateway bound.
		let longest = "a".repeat(63);
		assert_eq!(
			classify_labels(&mut all, &controller, [longest.as_str()]).await.unwrap(),
			identity(None, Some(&longest))
		);
		assert_eq!(classify_labels(&mut all, &controller, ["a".repeat(64)]).await.unwrap(), None);
		let oversized = format!("{}.01", "a".repeat(30));
		assert_eq!(classify_labels(&mut all, &controller, [oversized]).await.unwrap(), None);

		// One digit after the dot is not lite format; three digits neither.
		for wrong in ["alice.1", "alice.012"] {
			assert_eq!(classify_labels(&mut all, &controller, [wrong]).await.unwrap(), None);
		}
	});
}

#[test]
fn label_validators_follow_the_contract_rules() {
	assert!(is_full_person_label("alicebc"));
	assert!(is_full_person_label(&"a".repeat(32)));
	assert!(!is_full_person_label("web3-app"), "no digits or hyphens");
	assert!(!is_full_person_label("alice01"));
	assert!(!is_full_person_label("Alice"));
	assert!(!is_full_person_label(""));

	// PopRules: five letters or fewer are governance-reserved; longer
	// bases can be registered and reserved alike.
	assert!(is_registrable_full_label("george"));
	assert!(is_registrable_full_label("georgeabc"));
	assert!(is_registrable_full_label(&"a".repeat(32)));
	assert!(!is_registrable_full_label("alice"));
	assert!(!is_full_person_label("alice.bc"));
	assert!(!is_full_person_label("-alice"));
	assert!(!is_full_person_label(&"a".repeat(33)));

	assert!(is_dotted_lite_username("alice.01"));
	// The stem is letters only (`isLitePersonLabel`): digits and hyphens
	// in the stem are not lite labels.
	assert!(!is_dotted_lite_username("a2b.34"));
	assert!(!is_dotted_lite_username("a-b.34"));
	assert!(!is_dotted_lite_username("alice01"));
	assert!(!is_dotted_lite_username("alice.1"));
	assert!(!is_dotted_lite_username("alice.012"));
	assert!(!is_dotted_lite_username("a.lice.01"));
	assert!(!is_dotted_lite_username(".01"));
	// The 32-byte gateway bound applies to the dotted form: a 29-char stem
	// (32 bytes dotted) fits, a 30-char stem (33 bytes) does not.
	assert!(is_dotted_lite_username(&format!("{}.01", "a".repeat(29))));
	assert!(!is_dotted_lite_username(&format!("{}.01", "a".repeat(30))));
}

/// A transport for controller discovery: `DispatcherAddress` holds `stored`,
/// and `TARGET()` / `protocolRegistry()` answer with scripted results.
struct ScriptedDiscovery {
	stored: [u8; 20],
	target: fn() -> Result<Vec<u8>, EvmContractError>,
	protocol_registry: fn() -> Result<Vec<u8>, EvmContractError>,
}

#[async_trait::async_trait]
impl DotnsTransport for ScriptedDiscovery {
	async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		Ok(Some(self.stored.to_vec()))
	}

	async fn view(
		&mut self,
		_dest: &[u8; 20],
		input: Vec<u8>,
	) -> Result<Vec<u8>, EvmContractError> {
		if decoded::<RootGatewayDispatcher::TARGETCall>(&input).is_some() {
			(self.target)()
		} else if decoded::<DotnsPopController::protocolRegistryCall>(&input).is_some() {
			(self.protocol_registry)()
		} else if decoded::<DotnsPopController::pendingClaimsCall>(&input).is_some() {
			returns::<DotnsPopController::pendingClaimsCall>(Vec::new())
		} else {
			panic!("unscripted view {}", hex::encode(input));
		}
	}
}

fn address_answer(byte: u8) -> Result<Vec<u8>, EvmContractError> {
	returns::<RootGatewayDispatcher::TARGETCall>(Address::repeat_byte(byte))
}

#[test]
fn discovery_follows_target_when_the_stored_address_is_a_dispatcher() {
	let mut transport = ScriptedDiscovery {
		stored: [0xdd; 20],
		target: || address_answer(0xcc),
		protocol_registry: view_reverted,
	};
	let found =
		futures::executor::block_on(discover_pop_controller(&mut transport)).expect("discovery");
	assert_eq!(found, Some([0xcc; 20]), "TARGET() names the controller");
}

#[test]
fn discovery_uses_the_stored_address_once_the_pallet_points_at_the_controller() {
	let mut transport = ScriptedDiscovery {
		stored: [0xcc; 20],
		target: || panic!("must not probe the dispatcher view"),
		protocol_registry: || address_answer(0x9e),
	};
	let found =
		futures::executor::block_on(discover_pop_controller(&mut transport)).expect("discovery");
	assert_eq!(
		found,
		Some([0xcc; 20]),
		"protocolRegistry() answers, so the stored address is the controller"
	);
}

#[test]
fn discovery_reports_an_address_that_is_neither() {
	let mut transport = ScriptedDiscovery {
		stored: [0xab; 20],
		target: view_reverted,
		protocol_registry: view_reverted,
	};
	let err =
		futures::executor::block_on(discover_pop_controller(&mut transport)).expect_err("neither");
	assert!(err.to_string().contains("neither protocolRegistry() nor"), "{err}");
}

#[test]
fn discovery_separates_a_failed_confirmation_from_a_wrong_contract() {
	let mut transport = ScriptedDiscovery {
		stored: [0xdd; 20],
		target: || {
			Err(EvmContractError::Simulation(subxt::Error::other_str("node unreachable").into()))
		},
		protocol_registry: view_reverted,
	};
	let err =
		futures::executor::block_on(discover_pop_controller(&mut transport)).expect_err("failure");
	assert!(matches!(
		err,
		DotnsError::DispatcherTarget(DotnsCallError::View(EvmContractError::Simulation(reason)))
			if reason.to_string() == "Other error: node unreachable"
	));
}

#[test]
fn registry_reads_propagate_a_transport_failure_instead_of_guessing() {
	let mut transport = ScriptedDiscovery {
		stored: [0xcc; 20],
		target: || panic!("must not fall back on a transport failure"),
		protocol_registry: || {
			Err(EvmContractError::Simulation(subxt::Error::other_str("node unreachable").into()))
		},
	};
	for error in [
		futures::executor::block_on(discover_pop_controller(&mut transport)).unwrap_err(),
		futures::executor::block_on(resolve_labels(&mut transport, &[0xcc; 20], &[0xaa; 32]))
			.unwrap_err(),
	] {
		assert!(matches!(
			error,
			DotnsError::ProtocolRegistry(DotnsCallError::View(EvmContractError::Simulation(reason)))
				if reason.to_string() == "Other error: node unreachable"
		));
	}
}

/// A transport answering views by selector: `tld()` with a scripted
/// result, `get(bytes32)` with a registry address, `recordExists(bytes32)`
/// with a scripted bool.
struct ScriptedTld {
	tld: fn() -> Result<Vec<u8>, EvmContractError>,
	dot_record_exists: bool,
}

#[async_trait::async_trait]
impl DotnsTransport for ScriptedTld {
	async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		Ok(None)
	}

	async fn view(
		&mut self,
		_dest: &[u8; 20],
		input: Vec<u8>,
	) -> Result<Vec<u8>, EvmContractError> {
		if decoded::<ProtocolRegistry::tldCall>(&input).is_some() {
			(self.tld)()
		} else if decoded::<ProtocolRegistry::getCall>(&input).is_some() {
			returns::<ProtocolRegistry::getCall>(Address::repeat_byte(0xd0))
		} else if let Some(asked) = decoded::<DotnsRegistry::recordExistsCall>(&input) {
			assert_eq!(asked.node, tld_node(TLD_WITHOUT_VIEW));
			returns::<DotnsRegistry::recordExistsCall>(self.dot_record_exists)
		} else {
			panic!("unscripted view {}", hex::encode(input));
		}
	}
}

#[test]
fn the_tld_falls_back_only_when_the_view_reverts_and_the_record_exists() {
	let registry = [0x9e; 20];
	let reverted = view_reverted;

	let served = futures::executor::block_on(network_tld(
		&mut ScriptedTld {
			tld: || returns::<ProtocolRegistry::tldCall>(".paseo".to_string()),
			dot_record_exists: false,
		},
		&registry,
	));
	assert_eq!(served.unwrap(), ".paseo");

	// The fallback is verified: the registry must hold the ".dot" record.
	let verified = futures::executor::block_on(network_tld(
		&mut ScriptedTld { tld: reverted, dot_record_exists: true },
		&registry,
	));
	assert_eq!(verified.unwrap(), TLD_WITHOUT_VIEW);

	let unverified = futures::executor::block_on(network_tld(
		&mut ScriptedTld { tld: reverted, dot_record_exists: false },
		&registry,
	));
	assert!(unverified.is_err(), "{unverified:?}");

	// A transport failure is not an answer: no guessing.
	let failed = futures::executor::block_on(network_tld(
		&mut ScriptedTld {
			tld: || Err(EvmContractError::Simulation(subxt::Error::other_str("timed out").into())),
			dot_record_exists: true,
		},
		&registry,
	));
	assert!(failed.is_err(), "{failed:?}");
}

/// The full availability walk, scripted: tld → get(registrar) → exists.
struct ScriptedAvailability {
	exists: fn() -> Result<Vec<u8>, EvmContractError>,
}

#[async_trait::async_trait]
impl DotnsTransport for ScriptedAvailability {
	async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		Ok(None)
	}

	async fn view(
		&mut self,
		_dest: &[u8; 20],
		input: Vec<u8>,
	) -> Result<Vec<u8>, EvmContractError> {
		if decoded::<DotnsPopController::protocolRegistryCall>(&input).is_some() {
			returns::<DotnsPopController::protocolRegistryCall>(Address::repeat_byte(0x9e))
		} else if decoded::<ProtocolRegistry::tldCall>(&input).is_some() {
			returns::<ProtocolRegistry::tldCall>(".paseo".to_string())
		} else if decoded::<ProtocolRegistry::getCall>(&input).is_some() {
			returns::<ProtocolRegistry::getCall>(Address::repeat_byte(0xd1))
		} else if let Some(asked) = decoded::<DotnsRegistrar::existsCall>(&input) {
			let node = namehash_under(&tld_node(".paseo"), "george");
			assert_eq!(asked.tokenId, U256::from_be_bytes(node));
			(self.exists)()
		} else {
			panic!("unscripted view {}", hex::encode(input));
		}
	}
}

/// `available` must fail closed: a registrar that reverts (a wrong or
/// undeployed entry) is an error, never "available", or the already-minted
/// guard would pass for every name.
#[test]
fn availability_fails_closed_when_the_registrar_does_not_answer() {
	let controller = [0xc0; 20];
	let run = |exists: fn() -> Result<Vec<u8>, EvmContractError>| {
		futures::executor::block_on(label_available(
			&mut ScriptedAvailability { exists },
			&controller,
			"george",
		))
	};

	assert!(run(|| returns::<DotnsRegistrar::existsCall>(false)).unwrap());
	assert!(!run(|| returns::<DotnsRegistrar::existsCall>(true)).unwrap());
	let reverted = run(view_reverted);
	assert!(reverted.is_err(), "{reverted:?}");
	let failed =
		run(|| Err(EvmContractError::Simulation(subxt::Error::other_str("timed out").into())));
	assert!(failed.is_err(), "{failed:?}");
}

#[test]
fn store_labels_lose_the_network_tld_and_subnames_are_dropped() {
	assert_eq!(bare_store_label("alice01.paseo", ".paseo"), Some("alice01"));
	assert_eq!(bare_store_label("alice01.dot", ".dot"), Some("alice01"));
	// Wrong TLD, subname, or nothing left after the TLD.
	assert_eq!(bare_store_label("alice01.dot", ".paseo"), None);
	assert_eq!(bare_store_label("app.alice.paseo", ".paseo"), None);
	assert_eq!(bare_store_label(".paseo", ".paseo"), None);
	// A dotted lite label survives the strip; other dotted remainders
	// stay subnames.
	assert_eq!(bare_store_label("alice.01.paseo", ".paseo"), Some("alice.01"));
	assert_eq!(bare_store_label("sub.alice.01.paseo", ".paseo"), None);
	// An empty TLD leaves bare labels as they are.
	assert_eq!(bare_store_label("alice01", ""), Some("alice01"));
}

#[test]
fn storage_keys_have_the_expected_layout() {
	// Blake2_128Concat over the SCALE-encoded BaseLabel (compact length ‖ bytes).
	let owner_key = lite_label_owner_key(b"alice.01");
	let encoded = [&[0x20u8][..], b"alice.01"].concat();
	assert_eq!(
		&owner_key[..32],
		[twox_128(b"DotnsGateway").as_slice(), twox_128(b"LiteLabelOwner").as_slice(),].concat()
	);
	assert_eq!(&owner_key[32..48], &blake2_128(&encoded));
	assert_eq!(&owner_key[48..], &encoded);

	let alias_key = account_alias_key(&[0x11; 32]);
	let prefix =
		[twox_128(b"DotnsGateway").as_slice(), twox_128(b"AccountAlias").as_slice()].concat();
	assert_eq!(&alias_key[..32], prefix.as_slice());
	// Blake2_128Concat over the raw account bytes.
	assert_eq!(&alias_key[48..], &[0x11; 32]);

	assert_eq!(dispatcher_address_key().len(), 32);
	assert_eq!(timestamp_now_key().len(), 32);
}
