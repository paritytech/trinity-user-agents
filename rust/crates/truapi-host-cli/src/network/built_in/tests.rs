use clap::ValueEnum;

use super::{super::testing::edit_descriptor, *};

/// A bundled copy that no longer opens with its pointer would leave the
/// built-in network, or the check that tells a Polkadot fork from Polkadot,
/// with nothing to start from when the published copy is unreachable. A
/// built-in's descriptor `id` is its `--network` value, so the namespace a
/// user selects is the one its state is stored under.
#[test]
fn test_bundled_stack_pointers() {
	let built_ins = Network::value_variants()
		.iter()
		.map(|network| {
			let value = network.to_possible_value().unwrap();
			(network.built_in_stack(), value.get_name().to_owned())
		})
		.chain([(POLKADOT_REFERENCE_STACK, "polkadot".to_owned())]);
	for (built_in, id) in built_ins {
		let bundled = trinity_stack::open(built_in.bundle, &built_in.pointer())
			.unwrap_or_else(|error| panic!("{id}: {error}"));
		assert!(bundled.signed);
		assert_eq!(bundled.descriptor.id, id);
	}
}

/// `stack bundle` writes the copy every command starts from, so a built-in
/// network's copy the CLI cannot run on is not written; any other stack is
/// bundled as fetched.
#[test]
fn test_built_in_runnable_check() {
	let without_dub = super::super::testing::edit_stack(|descriptor| {
		descriptor["services"]
			.as_array_mut()
			.unwrap()
			.retain(|service| service["kind"] != "dub");
	});
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();
	let error = ensure_built_in_runnable(&paseo, &without_dub).unwrap_err();
	assert!(error.to_string().contains("no dub service"));
	ensure_built_in_runnable(&POLKADOT_REFERENCE_STACK.pointer(), &without_dub)
		.expect("a stack other than a built-in network checked");
	ensure_built_in_runnable(&paseo, &Network::PaseoNextV2.built_in_stack().open_bundled())
		.expect("bundled paseo-next-v2 copy not runnable");
}

/// The hint tells the user which copy is in use and why, so a restart on
/// the bundled copy points at the published descriptor that could not be
/// fetched or was rejected.
#[test]
fn test_restart_hint_by_copy() {
	let url = "https://stack.example/s.json".to_owned();
	let published =
		BuiltInOrigin { id: "previewnet".to_owned(), url, copy: BuiltInCopy::Published };
	assert_eq!(
		published.restart_hint(),
		"the previewnet stack restarted from genesis after the published descriptor in use \
		 (https://stack.example/s.json) was published"
	);
	let unreachable = BuiltInOrigin { copy: BuiltInCopy::Unreachable, ..published };
	assert_eq!(
		unreachable.restart_hint(),
		"the previewnet stack restarted from genesis after the bundled descriptor in use was \
		 built, and the published copy at https://stack.example/s.json could not be fetched"
	);
	let rejected = BuiltInOrigin { copy: BuiltInCopy::Rejected, ..unreachable };
	assert_eq!(
		rejected.restart_hint(),
		"the previewnet stack restarted from genesis after the bundled descriptor in use was \
		 built, and the published copy at https://stack.example/s.json was rejected, as the \
		 warning above reports"
	);
}

const TEST_SECRET: [u8; 32] = [7; 32];

fn test_pointer() -> Pointer {
	let key = format!("0x{}", hex::encode(trinity_stack::public_key(&TEST_SECRET)));
	Pointer::new("https://stack.example/trinity-stack.json", Some(&key)).unwrap()
}

fn sign_copy(sequence: u32, generation: &str) -> Vec<u8> {
	sign_edited(|descriptor| {
		descriptor["sequence"] = sequence.into();
		descriptor["generation"] = generation.into();
	})
}

fn sign_edited(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
	trinity_stack::sign(&edit_descriptor(edit), &TEST_SECRET).unwrap()
}

fn refresh(bundled: &[u8], fetched: Fetched) -> Result<StackRefresh> {
	refresh_stack("test", &test_pointer(), bundled, fetched, NOW, ChainBackend::Rpc)
}

const NOW: i64 = 1_800_000_000;

/// A newer published copy that changes a field the CLI reads is what the
/// network runs on, and the user learns that it is in use ahead of the
/// release that ships it.
#[test]
fn test_refresh_newer_notice() {
	let remote_copy = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		let people = descriptor["chains"]
			.as_array_mut()
			.unwrap()
			.iter_mut()
			.find(|chain| chain["role"] == "People")
			.unwrap();
		people["rpc"] = serde_json::json!(["wss://people.example"]);
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&remote_copy)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 11);
	assert_eq!(refresh.copy, BuiltInCopy::Published);
	let notice = refresh.notice.expect("newer copy not reported");
	assert_eq!((notice.bundle_sequence, notice.remote_sequence), (10, 11));
	assert!(!notice.new_generation);
	let text = notice.to_string();
	assert!(text.contains("10") && text.contains("11") && text.contains("in use"));
}

/// A republish that changes only fields the CLI does not read, such as a
/// higher sequence, is used without a notice.
#[test]
fn test_refresh_sequence_only_silent() {
	let remote_copy = sign_copy(11, "g1");
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&remote_copy)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 11);
	assert_eq!(refresh.copy, BuiltInCopy::Published);
	assert!(refresh.notice.is_none());
}

/// The chains behind the bundled copy are gone after a restart, which the
/// notice reports beyond a routine update.
#[test]
fn test_refresh_new_generation_notice() {
	let published = sign_copy(11, "g2");
	let notice = refresh(&sign_copy(10, "g1"), Fetched::Document(&published))
		.unwrap()
		.notice
		.expect("new generation not reported");
	assert!(notice.new_generation);
	assert!(notice.to_string().contains("restarted from genesis"));
}

/// Accounts derive under the TLD, so a refresh that changes it would swap
/// every account under the user.
#[test]
fn test_refresh_tld_change_bundle() {
	let other_tld_copy = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		descriptor["tld"] = "testnet".into();
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&other_tld_copy)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 10);
	assert_eq!(refresh.stack.tld(), "paseo");
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

/// The id is the namespace every session and account is stored under, so a
/// refresh that changes it would move them all.
#[test]
fn test_refresh_id_change_bundle() {
	let renamed_copy = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		descriptor["id"] = "paseo-next".into();
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&renamed_copy)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 10);
	assert_eq!(refresh.stack.namespace(), "paseo-next-v2");
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

/// A published copy the CLI cannot run on, such as one without a `dub`
/// service, leaves the network on the bundled copy instead of stopping every
/// command until a release ships a fix.
#[test]
fn test_refresh_unrunnable_copy_bundle() {
	let without_dub = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g1".into();
		descriptor["services"]
			.as_array_mut()
			.unwrap()
			.retain(|service| service["kind"] != "dub");
	});
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&without_dub)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 10);
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

/// A signed copy with a lower sequence is a replay of an older descriptor.
#[test]
fn test_refresh_rollback_bundle() {
	let older = sign_copy(9, "g0");
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Document(&older)).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 10);
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Rejected);
}

#[test]
fn test_refresh_unreachable_bundle() {
	let refresh = refresh(&sign_copy(10, "g1"), Fetched::Unreachable).unwrap();
	assert_eq!(refresh.stack.descriptor().sequence, 10);
	assert!(refresh.notice.is_none());
	assert_eq!(refresh.copy, BuiltInCopy::Unreachable);
}

/// An expired stack has shut down, from either copy.
#[test]
fn test_refresh_expired_error() {
	let expired = |sequence: u32| {
		sign_edited(|descriptor| {
			descriptor["sequence"] = sequence.into();
			descriptor["expiresAt"] = (NOW as u32 - 1).into();
		})
	};
	let error = refresh(&expired(10), Fetched::Unreachable)
		.err()
		.expect("expired stack accepted");
	assert!(error.to_string().contains("expiresAt"));

	let published = expired(11);
	assert!(refresh(&sign_copy(10, "g1"), Fetched::Document(&published)).is_err());
}

/// A product sees the same network, TLD and chains from the CLI as from an
/// app bundling the same signed copy: the descriptor's `network`, not the
/// CLI's product namespace. The CLI serves no Relay, so it names none.
#[test]
fn the_core_stack_reports_what_an_app_bundling_it_reports() {
	for network in Network::value_variants() {
		let built_in = network.built_in_stack();
		let allowed = truapi::platform::ReservedValuesPolicy::ALLOWED;
		let bundled =
			truapi::platform::Stack::bundled(built_in.link.trim(), built_in.bundle, &allowed)
				.expect("the bundled copy starts an app");
		let expected = truapi::platform::Stack::new(
			bundled.network().to_string(),
			bundled.tld().to_string(),
			truapi::platform::StackChains { relay: None, ..bundled.chains().clone() },
			&allowed,
		);

		assert_eq!(network.bundled_stack(ChainBackend::Rpc).core_stack(), expected, "{network:?}");
	}
}
