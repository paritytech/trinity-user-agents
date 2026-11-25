use clap::ValueEnum;

use super::{
	super::testing::{edit_descriptor, test_access},
	*,
};
use crate::chain::ChainBackend;

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

/// A built-in's copy runs on either chain backend, so one without a light
/// spec is not written: every light-client command would fail on it. Any
/// other stack may run over RPC only, so this check accepts it without one.
#[test]
fn test_built_in_runnable_check_on_the_light_client() {
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();
	let url = paseo.url();
	for role in ["Relay", "People", "AssetHub", "Bulletin"] {
		let without_spec = super::super::testing::edit_chain(role, |chain| {
			chain.as_object_mut().unwrap().remove("lightSpec");
		});
		let error = ensure_built_in_runnable(&paseo, &without_spec).unwrap_err();
		assert_eq!(
			error.to_string(),
			format!(
				"{url} is a built-in network's stack the CLI cannot run on the light client: its \
				 descriptor names no lightSpec for {role}"
			)
		);
		ensure_built_in_runnable(&POLKADOT_REFERENCE_STACK.pointer(), &without_spec)
			.expect("a stack other than a built-in network checked for its light specs");
	}
}

/// `stack bundle` refuses a built-in's copy by this check and a light-client
/// command refuses it by `Stack::new`, each with its own list of the chains
/// that need a light spec. They give the same answer for a copy that lacks one
/// chain's light spec, for every chain of every built-in, so this check
/// refuses such a copy exactly when a light-client command would.
#[test]
fn test_built_in_runnable_check_refuses_what_the_light_client_refuses() {
	let light_client = test_access(ChainBackend::LightClient);
	let mut refused_by_check = Vec::new();
	let mut refused_by_light_client = Vec::new();
	for network in Network::value_variants() {
		let built_in = network.built_in_stack();
		let bundled = built_in.open_bundled();
		for (index, chain) in bundled.chains.iter().enumerate() {
			let mut document = serde_json::to_value(&bundled).expect("descriptor not JSON");
			let edited = document["chains"][index].as_object_mut().expect("a chain is an object");
			edited.remove("lightSpec");
			let without_spec = Descriptor::from_json(document).expect("invalid edited descriptor");

			let refused = ensure_built_in_runnable(&built_in.pointer(), &without_spec).is_err();
			refused_by_check.push((*network, chain.role.clone(), refused));
			let refused =
				Stack::new(built_in.pointer().stack_id(), without_spec, &light_client).is_err();
			refused_by_light_client.push((*network, chain.role.clone(), refused));
		}
	}

	assert_eq!(refused_by_check, refused_by_light_client);
}

/// A chain the light client does not run needs no light spec, so this check
/// and a light-client command both accept a built-in's copy that names none
/// for it. The chain is added here because the bundled copies may stop
/// listing one.
#[test]
fn test_built_in_runnable_check_accepts_a_chain_the_light_client_does_not_run() {
	let with_unrouted_chain = super::super::testing::edit_stack(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().unwrap();
		chains.push(serde_json::json!({
			"role": "Unrouted",
			"genesisHash": format!("0x{}", "11".repeat(32)),
			"paraId": u32::MAX,
			"rpc": ["wss://unrouted.example"],
		}));
	});
	let paseo = Network::PaseoNextV2.built_in_stack().pointer();

	let by_check =
		ensure_built_in_runnable(&paseo, &with_unrouted_chain).map_err(|error| error.to_string());
	let by_light_client =
		Stack::new(paseo.stack_id(), with_unrouted_chain, &test_access(ChainBackend::LightClient))
			.map(drop)
			.map_err(|error| error.to_string());

	assert_eq!((by_check, by_light_client), (Ok(()), Ok(())));
}

/// A light client on a fresh state directory starts from the specs this build
/// ships, so every spec a built-in's bundled copy names for a chain it runs
/// must be shipped and pass the check. `scripts/update-stacks.sh` refreshes
/// both together.
#[test]
fn test_bundled_light_specs_match_the_bundled_copies() {
	for network in Network::value_variants() {
		let descriptor = network.built_in_stack().open_bundled();
		for chain in crate::stack_descriptor::light_client_chains(&descriptor) {
			let named = chain.light_spec.as_ref().expect("a built-in names every light spec");
			let spec = bundled_light_spec(&named.sha256)
				.unwrap_or_else(|| panic!("{network:?} ships no light spec for {}", chain.role));
			chain
				.check_light_spec(spec.as_bytes())
				.unwrap_or_else(|error| panic!("{network:?} {}: {error}", chain.role));
		}
	}
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
		"the previewnet stack restarted from genesis after the descriptor in use was published, \
		 and the published copy at https://stack.example/s.json could not be fetched"
	);
	let rejected = BuiltInOrigin { copy: BuiltInCopy::Rejected, ..unreachable };
	assert_eq!(
		rejected.restart_hint(),
		"the previewnet stack restarted from genesis after the descriptor in use was published, \
		 and the published copy at https://stack.example/s.json was rejected, as the warning \
		 above reports"
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
	refresh_on(&ChainAccess::Rpc, bundled, fetched)
}

fn refresh_on(
	chain_access: &ChainAccess,
	bundled: &[u8],
	fetched: Fetched,
) -> Result<StackRefresh> {
	let name = "published test stack descriptor";
	refresh_stack("test", name, &test_pointer(), bundled, fetched, NOW, chain_access)
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

/// The light client runs a newer published copy from the light specs it
/// names. One that names none for a chain it runs is an error rather than a
/// fall back to the bundled copy, whose chains may have restarted since; over
/// RPC the same copy runs.
#[test]
fn test_refresh_on_the_light_client() {
	let light_client = test_access(ChainBackend::LightClient);
	let bundled = sign_copy(10, "g1");
	let restarted = sign_copy(11, "g2");
	let published = refresh_on(&light_client, &bundled, Fetched::Document(&restarted)).unwrap();
	assert_eq!(published.stack.descriptor().sequence, 11);
	assert!(published.stack.light_client().is_some());

	let without_people_spec = sign_edited(|descriptor| {
		descriptor["sequence"] = 11.into();
		descriptor["generation"] = "g2".into();
		let chains = descriptor["chains"].as_array_mut().unwrap();
		let people = chains.iter_mut().find(|chain| chain["role"] == "People").unwrap();
		people.as_object_mut().unwrap().remove("lightSpec");
	});
	let error = refresh_on(&light_client, &bundled, Fetched::Document(&without_people_spec))
		.err()
		.expect("a published copy without People's light spec ran on the light client");
	assert!(format!("{error:#}").contains("no lightSpec for People"), "{error:#}");

	let over_rpc = refresh(&bundled, Fetched::Document(&without_people_spec)).unwrap();
	assert_eq!(over_rpc.stack.descriptor().sequence, 11);
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
		let link = built_in.pointer().to_link(trinity_stack::DEFAULT_LINK_BASE);
		let allowed = truapi::platform::ReservedValuesPolicy::ALLOWED;
		let bundled = truapi::platform::Stack::bundled(&link, built_in.bundle, &allowed)
			.expect("the bundled copy starts an app");
		let identity = truapi::platform::StackIdentity {
			id: bundled.id().to_string(),
			generation: bundled.generation().to_string(),
		};
		let expected = truapi::platform::Stack::new(
			identity,
			bundled.network().to_string(),
			bundled.tld().to_string(),
			truapi::platform::StackChains { relay: None, ..bundled.chains().clone() },
			bundled.sequence(),
			&allowed,
		);

		assert_eq!(network.bundled_stack(ChainBackend::Rpc).core_stack(), expected, "{network:?}");
	}
}

/// A built-in stack is signed, so its bare URL has to resolve to the same
/// stack as its network name. Read as an unsigned stack it would get another
/// id, and the core would find none of the state stored under the name.
#[test]
fn a_built_in_stack_named_by_its_url_keeps_its_key() {
	for network in Network::value_variants() {
		let pointer = network.built_in_stack().pointer();
		let key = pointer.key().map(|key| format!("0x{}", hex::encode(key)));
		let link = pointer.to_link(trinity_stack::DEFAULT_LINK_BASE);

		assert_eq!(
			(built_in_key(pointer.url().as_str()), built_in_key(&link)),
			(key, None),
			"{network:?}"
		);
	}
	assert_eq!(built_in_key("https://stack.test.invalid/other.json"), None);
	assert_eq!(built_in_key("./stack.json"), None);
}

/// After a wipe the bundled copy names chains that are gone. A host that ran
/// on the published descriptor since must not go back to them when that
/// descriptor is unreachable: the core would take the older run for a wipe
/// and drop what the session read from the chains. The published copy still
/// wins once it arrives, a kept copy older than the bundled one is not used,
/// and neither a published copy older than the kept one nor one the CLI
/// refuses, here for a changed TLD, replaces it.
#[test]
fn an_unreachable_built_in_starts_from_the_descriptor_a_host_kept() {
	let bundled = sign_copy(10, "g1");
	let (after_wipe, republished, outdated) =
		(sign_copy(11, "g2"), sign_copy(12, "g2"), sign_copy(9, "g0"));
	let rollback = sign_copy(10, "g1");
	let other_tld = sign_edited(|descriptor| {
		descriptor["sequence"] = 12.into();
		descriptor["generation"] = "g2".into();
		descriptor["tld"] = "testnet".into();
	});
	let run = |published: Option<&[u8]>, last_run: Option<&[u8]>| {
		let copies = Copies { bundled: &bundled, published, last_run };
		let refresh =
			refresh_from_last_run("test", &test_pointer(), &copies, NOW, &ChainAccess::Rpc)
				.expect("a descriptor is usable");
		(refresh.stack.descriptor().sequence, refresh.copy)
	};

	assert_eq!(
		[
			run(None, Some(&after_wipe)),
			run(Some(&republished), Some(&after_wipe)),
			run(None, Some(&outdated)),
			run(None, None),
			run(Some(&rollback), Some(&after_wipe)),
			run(Some(&other_tld), Some(&after_wipe)),
		],
		[
			(11, BuiltInCopy::Unreachable),
			(12, BuiltInCopy::Published),
			(10, BuiltInCopy::Unreachable),
			(10, BuiltInCopy::Unreachable),
			(11, BuiltInCopy::Rejected),
			(11, BuiltInCopy::Rejected),
		]
	);
}

/// A copies directory, and the path in it a host keeps the test stack's
/// descriptor at, holding `kept` when given.
fn copies_directory(kept: Option<&[u8]>) -> (tempfile::TempDir, std::path::PathBuf) {
	let base = tempfile::tempdir().expect("a temporary directory");
	let path = base.path().join("stacks").join("test.json");
	if let Some(kept) = kept {
		fs::create_dir_all(base.path().join("stacks")).expect("the copies directory");
		fs::write(&path, kept).expect("the kept copy");
	}
	(base, path)
}

/// What the next run reads from the copies directory at `path`.
fn read_back(path: &Path) -> Option<Vec<u8>> {
	read_kept_copy(path).expect("the kept copy reads")
}

/// A run of a host on the bundled copy of sequence 10, with `published`
/// fetched when given and `kept` in its copies directory: the sequence it runs
/// on, which copy that is, and what the next run reads from the directory.
fn run_and_keep(
	published: Option<&[u8]>,
	kept: Option<&[u8]>,
) -> (u32, BuiltInCopy, Option<Vec<u8>>) {
	let (_base, path) = copies_directory(kept);
	let bundled = sign_copy(10, "g1");
	let refresh = refresh_with_kept_copy(
		"test",
		&test_pointer(),
		&bundled,
		published,
		Some(&path),
		NOW,
		&ChainAccess::Rpc,
	)
	.expect("a descriptor is usable");
	(refresh.stack.descriptor().sequence, refresh.copy, read_back(&path))
}

/// The copy a host keeps has to be there for its next run, which starts from
/// it when the published descriptor is unreachable. A host that has kept
/// nothing reads none, its first copy creates the directory, and a later one
/// replaces it.
#[test]
fn a_kept_copy_is_read_back_by_the_next_run() {
	let (_base, path) = copies_directory(None);
	let (published, republished) = (sign_copy(11, "g2"), sign_copy(12, "g2"));
	let before = read_back(&path);

	keep_copy(&path, &test_pointer(), &published, NOW);
	let kept = read_back(&path);
	keep_copy(&path, &test_pointer(), &republished, NOW);

	assert_eq!((before, kept, read_back(&path)), (None, Some(published), Some(republished)));
}

/// A kept copy that is not a descriptor, such as one cut short, or that is
/// larger than any descriptor the CLI fetches, is not one the host kept. The
/// host passes it over, starting from the bundled copy while the published
/// one is unreachable, and keeps the published one over it once that arrives.
/// A file that large is not read.
#[test]
fn a_corrupt_kept_copy_is_passed_over_and_replaced() {
	let published = sign_copy(11, "g2");
	let cut_short = published[..published.len() / 2].to_vec();
	let oversized = vec![b' '; MAX_FETCH_BYTES + 1];

	assert_eq!(
		[
			run_and_keep(None, Some(&cut_short)),
			run_and_keep(Some(&published), Some(&cut_short)),
			run_and_keep(None, Some(&oversized)),
			run_and_keep(Some(&published), Some(&oversized)),
		],
		[
			(10, BuiltInCopy::Unreachable, Some(cut_short.clone())),
			(11, BuiltInCopy::Published, Some(published.clone())),
			(10, BuiltInCopy::Unreachable, None),
			(11, BuiltInCopy::Published, Some(published.clone())),
		]
	);
}

/// Log lines written into a shared buffer.
struct Captured(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl io::Write for Captured {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		self.0.lock().unwrap().extend_from_slice(bytes);
		Ok(bytes.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

/// A kept copy too large to be a descriptor is reported once per resolve, not
/// again when the host keeps the published copy over it.
#[test]
fn an_oversized_kept_copy_is_reported_once() {
	let lines = std::sync::Arc::<std::sync::Mutex<Vec<u8>>>::default();
	let subscriber = tracing_subscriber::fmt()
		.with_writer({
			let lines = std::sync::Arc::clone(&lines);
			move || Captured(lines.clone())
		})
		.with_ansi(false)
		.finish();
	let (published, oversized) = (sign_copy(11, "g2"), vec![b' '; MAX_FETCH_BYTES + 1]);

	let run = tracing::subscriber::with_default(subscriber, || {
		run_and_keep(Some(&published), Some(&oversized))
	});

	let logged = String::from_utf8(lines.lock().unwrap().clone()).expect("log lines are text");
	assert_eq!(
		(run, logged.matches("is larger than").count()),
		((11, BuiltInCopy::Published, Some(published.clone())), 1)
	);
}

/// A kept copy that cannot be read stops the command, also when the published
/// copy arrives and would be taken: the kept copy is what tells a published
/// copy served stale from a current one, so without it the host could go back
/// to chains a wipe replaced. The error names the file and how to recover.
#[test]
fn a_kept_copy_that_cannot_be_read_stops_the_command() {
	let (_base, path) = copies_directory(None);
	fs::create_dir_all(&path).expect("a directory where the copy goes");
	let published = sign_copy(11, "g2");
	let stopped_with = |published: Option<&[u8]>| {
		let bundled = sign_copy(10, "g1");
		refresh_with_kept_copy(
			"test",
			&test_pointer(),
			&bundled,
			published,
			Some(&path),
			NOW,
			&ChainAccess::Rpc,
		)
		.err()
		.map(|error| error.to_string())
	};
	let recovery = format!(
		"could not read the stack descriptor kept at {}; fix its permissions, or remove it to \
		 start from the published or the bundled copy",
		path.display()
	);

	assert_eq!(
		[stopped_with(None), stopped_with(Some(&published))],
		[Some(recovery.clone()), Some(recovery)]
	);
}

/// Another host holds the lock on the kept copy, as the test does here, until
/// `released` fires or the test ends. The lock file is `path` with `.lock`
/// added.
fn hold_lock(path: &Path, released: std::sync::mpsc::Receiver<()>) {
	let lock = fs::File::create(path.with_extension("json.lock")).expect("the lock file");
	lock.lock_exclusive().expect("another host holds the lock");
	std::thread::spawn(move || {
		let _ = released.recv_timeout(std::time::Duration::from_secs(10));
		drop(lock);
	});
}

/// `published` kept at `path` from another thread: whether keeping it ended
/// within five seconds, and how. That is well past the second a host waits
/// for the lock and short of the ten a [`hold_lock`] holder keeps it.
fn kept_within_deadline(path: &Path, published: Vec<u8>) -> Option<Result<(), String>> {
	let (kept_it, keeping) = std::sync::mpsc::channel();
	std::thread::spawn({
		let path = path.to_path_buf();
		move || {
			let kept = replace_older_copy(&path, &test_pointer(), &published, NOW);
			let _ = kept_it.send(kept.map_err(|error| format!("{error:#}")));
		}
	});
	keeping.recv_timeout(std::time::Duration::from_secs(5)).ok()
}

/// Keeping the copy is best effort. A host that finds the lock held for
/// longer than a moment, as by another host stopped in a debugger, starts
/// without keeping its copy instead of waiting for the lock, and that is no
/// failure.
#[test]
fn a_host_does_not_wait_for_a_held_lock_to_keep_its_copy() {
	let kept = sign_copy(11, "g2");
	let (_base, path) = copies_directory(Some(&kept));
	let (_release, released) = std::sync::mpsc::channel();
	hold_lock(&path, released);

	let keeping = kept_within_deadline(&path, sign_copy(12, "g2"));

	assert_eq!((keeping, read_back(&path)), (Some(Ok(())), Some(kept)));
}

/// Two hosts that start together contend for the lock for as long as one
/// takes to keep its copy. The other waits for it, so a later copy it ran on
/// is still kept, and its next offline start does not go back to the earlier
/// one.
#[test]
fn a_host_keeps_its_copy_once_a_brief_lock_is_released() {
	let (_base, path) = copies_directory(Some(&sign_copy(11, "g2")));
	let (release, released) = std::sync::mpsc::channel();
	hold_lock(&path, released);
	std::thread::spawn(move || {
		std::thread::sleep(std::time::Duration::from_millis(200));
		let _ = release.send(());
	});
	let later = sign_copy(12, "g2");

	let keeping = kept_within_deadline(&path, later.clone());

	assert_eq!((keeping, read_back(&path)), (Some(Ok(())), Some(later)));
}

/// A published copy the CLI does not run, here for a changed TLD, leaves the
/// copy a host kept in use. The bundled copy is not the fall back then, so a
/// bundled copy past its `expiresAt` does not stop a host that has a later
/// one.
#[test]
fn a_kept_copy_stands_in_for_a_published_copy_the_cli_does_not_run() {
	let expired_bundle = sign_edited(|descriptor| {
		descriptor["sequence"] = 10.into();
		descriptor["generation"] = "g1".into();
		descriptor["expiresAt"] = (NOW as u32 - 1).into();
	});
	let other_tld = sign_edited(|descriptor| {
		descriptor["sequence"] = 12.into();
		descriptor["generation"] = "g2".into();
		descriptor["tld"] = "testnet".into();
	});
	let kept = sign_copy(11, "g2");
	let copies =
		Copies { bundled: &expired_bundle, published: Some(&other_tld), last_run: Some(&kept) };

	let refresh = refresh_from_last_run("test", &test_pointer(), &copies, NOW, &ChainAccess::Rpc)
		.map(|refresh| (refresh.stack.descriptor().sequence, refresh.copy))
		.map_err(|error| error.to_string());

	assert_eq!(refresh, Ok((11, BuiltInCopy::Rejected)));
}

/// A CLI update can ship a later descriptor than the one the host kept. That
/// is no rollback to warn about, so the kept copy is passed over before the
/// refresh rules see it. A copy that does not verify is left to them, which
/// say why it is not used.
#[test]
fn a_kept_copy_older_than_the_bundled_one_is_passed_over() {
	let bundled = sign_copy(10, "g1");
	let older = |last_run: &[u8]| is_older_than_bundled(&test_pointer(), &bundled, last_run, NOW);

	assert_eq!(
		[
			older(&sign_copy(9, "g0")),
			older(&sign_copy(11, "g2")),
			older(&bundled),
			older(b"not a descriptor")
		],
		[true, false, false, false]
	);
}

/// A host keeps the published copy it runs on, and only that one, so a copy
/// it did not take never becomes the copy its next offline run starts from.
/// Nor does it keep one over a later copy, such as the one another host
/// starting at the same time kept after this one read the file.
#[test]
fn only_a_published_copy_in_use_replaces_the_kept_one() {
	let (kept, published, later) = (sign_copy(11, "g2"), sign_copy(12, "g2"), sign_copy(13, "g2"));
	let other_tld = sign_edited(|descriptor| {
		descriptor["sequence"] = 12.into();
		descriptor["generation"] = "g2".into();
		descriptor["tld"] = "testnet".into();
	});
	let kept_after =
		|published: Option<&[u8]>, kept: Option<&[u8]>| run_and_keep(published, kept).2;
	let (_base, raced) = copies_directory(Some(&later));
	keep_copy(&raced, &test_pointer(), &published, NOW);

	assert_eq!(
		[
			kept_after(Some(&published), None),
			kept_after(Some(&published), Some(&kept)),
			kept_after(Some(&other_tld), None),
			kept_after(Some(&other_tld), Some(&kept)),
			kept_after(None, Some(&kept)),
			read_back(&raced),
		],
		[
			Some(published.clone()),
			Some(published),
			None,
			Some(kept.clone()),
			Some(kept),
			Some(later),
		]
	);
}
