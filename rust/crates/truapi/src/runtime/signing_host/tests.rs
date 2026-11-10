mod activation;
mod allowance_keys;
mod auto_signing;
mod cross_product_account;
#[cfg(feature = "test-host")]
mod local_preimages;
mod raw_signing;
mod remote_consent;
#[cfg(feature = "test-host")]
mod withheld_resources;

use std::sync::Arc;

use super::{
	super::{
		AccountHolder, HostSession, ProductAuthority, ProductRuntimeHost, RuntimeServices,
		SigningHostRole,
		authority::{
			AuthorityError, AuthoritySession, CreateTransactionAuthorityRequest,
			SignPayloadAuthorityRequest, SignRawAuthorityRequest, StatementStoreAllowanceKey,
		},
	},
	LocalActivation, TEST_NETWORK_SUFFIX,
	ring_vrf::{MemberCandidate, ResolvedRing, RingResolver},
};
use crate::{
	Spawner,
	host_internal::{
		extrinsic::tests::split_v4,
		sso_messages::{ProductRequest, RingVrfError},
		transaction::{extrinsic_payload_extensions, extrinsic_payload_preimage},
	},
	host_logic::product_account::{
		SR25519_SIGNING_CONTEXT, derive_identity_keypair, derive_product_keypair,
		derive_ring_vrf_entropy, derive_root_keypair_from_entropy, index_bytes,
	},
	platform::{
		CoreStorageKey, HostInfo, Platform, PlatformInfo, ProductContext, ProductId,
		SigningHostConfig,
	},
	test_support::{StubPlatform, test_storage, test_storage_key},
};
use truapi::{
	CallContext, CallError,
	api::{Account, Entropy, ResourceAllocation, Signing},
	latest::{
		HostAccountCreateProofRequest, HostAccountGetAliasRequest,
		HostAccountRegisterRingVrfKeyRequest, HostAccountRingVrfSignRequest,
	},
	v01,
	versioned::{
		account::{HostAccountGetError, HostAccountGetRequest},
		entropy::HostDeriveEntropyRequest,
		resource_allocation::{
			HostRequestResourceAllocationRequest, HostRequestResourceAllocationResponse,
		},
		signing::{HostSignRawError, HostSignRawRequest, HostSignRawResponse},
	},
};

const ENTROPY: [u8; 16] = [0xAB; 16];

#[derive(Clone)]
struct StubRingResolver {
	/// Holds the next ring validation until the test releases it.
	validation_gate: Arc<std::sync::Mutex<Option<futures::channel::oneshot::Receiver<()>>>>,
	collection: [u8; 32],
	ring: ResolvedRing,
}

#[async_trait::async_trait]
impl RingResolver for StubRingResolver {
	async fn members_pallet_index(&self, _chain_id: &[u8; 32]) -> Result<u8, RingVrfError> {
		Ok(42)
	}

	async fn validate(&self, _location: &v01::RingLocation) -> Result<[u8; 32], RingVrfError> {
		let gate = self.validation_gate.lock().unwrap().take();
		if let Some(gate) = gate {
			gate.await.unwrap();
		}
		Ok(self.collection)
	}

	async fn resolve(
		&self,
		_location: &v01::RingLocation,
		candidates: &[MemberCandidate],
	) -> Result<ResolvedRing, RingVrfError> {
		assert!(
			candidates.contains(&self.ring.selected),
			"signing host offered the explicitly registered key"
		);
		Ok(self.ring.clone())
	}
}

/// #660 and #655 join here: the hash the signing role installs is the one
/// the grant path adjudicates against.
///
/// Worth pinning because the two halves are testable apart and were built
/// apart. #660's own tests prove the hash is installed; #655's grant tests
/// seed the manifest **cache**, and `root_manifest` reads the cache before
/// it ever needs a genesis hash, so every one of them would pass with
/// #660 absent. This asserts the seam itself: the grant path's chain
/// lookup has an Asset Hub to run against on a role whose config used to
/// carry none.
///
/// A cache miss still refuses, because the stub reaches no chain. That is
/// the closed default, and it is why this seam needs its own test rather
/// than being visible in a refusal.
#[test]
fn the_signing_role_adjudicates_grants_against_the_asset_hub_it_installed() {
	let (services, _authority) =
		signing_runtime_on_asset_hub(Arc::new(StubPlatform::default()), Some([0xcc; 32]));
	assert_eq!(
		services.asset_hub_chain_genesis_hash(),
		Some([0xcc; 32]),
		"#660 must install the config's Asset Hub, or #655 resolves no manifest here"
	);

	let platform = Arc::new(StubPlatform::default());
	let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
		&services,
		platform.as_ref(),
		&ProductId::for_test("dim2.dot"),
		&ProductId::for_test("peopl.dot"),
		crate::host_internal::product_manifest::Granted::Context,
	));
	// Documentation, not a guard, and labelled so nobody reads it as one:
	// with no cached manifest and no reachable chain this is false whether
	// or not #660 installed a hash, so no mutation of the production path
	// can turn it red. The load-bearing assertion in this test is the one
	// above; the granted path is guarded by
	// `a_context_grant_lets_a_foreign_product_prove_with_the_owners_key`
	// and its M14 pair.
	assert!(!granted, "the closed default: no manifest reachable means no grant");
}

fn signing_runtime() -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
	// Auto-confirm raw signing so the role-neutral confirmation gate does
	// not reject before reaching the signing authority.
	let platform: Arc<dyn Platform> = Arc::new(StubPlatform {
		sign_raw_confirmed: true,
		sign_vrf_confirmed: true,
		..StubPlatform::default()
	});
	signing_runtime_with_platform(platform)
}

/// No dotNS Asset Hub, so activation looks up no username on the scripted chain.
fn signing_runtime_with_platform(
	platform: Arc<dyn Platform>,
) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
	signing_runtime_on_asset_hub(platform, None)
}

fn signing_runtime_on_asset_hub(
	platform: Arc<dyn Platform>,
	asset_hub_chain_genesis_hash: Option<[u8; 32]>,
) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
	let config = SigningHostConfig::new(
		HostInfo {
			name: "Polkadot Mobile".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Ios,
		},
		PlatformInfo::default(),
		crate::test_support::test_stack(
			TEST_NETWORK_SUFFIX,
			[0; 32],
			[0xbb; 32],
			asset_hub_chain_genesis_hash,
		),
	)
	.expect("signing host config is valid");
	let services = RuntimeServices::new(
		platform.clone(),
		config.host.host_info.clone(),
		config.stack.clone(),
		Spawner::for_test(),
	);
	let signing_host = SigningHostRole::new(services.clone());
	(services, signing_host)
}

fn product_runtime(
	services: Arc<RuntimeServices>,
	authority: Arc<SigningHostRole>,
) -> ProductRuntimeHost {
	ProductRuntimeHost::from_services(
		services.clone(),
		crate::host_core::ConnectionAdapters::from_services(&services),
		authority.clone(),
		authority,
		ProductContext::new(ProductId::for_test("myapp.dot")),
	)
}

fn product_runtime_for(
	services: Arc<RuntimeServices>,
	authority: Arc<SigningHostRole>,
	product_id: &str,
) -> ProductRuntimeHost {
	ProductRuntimeHost::from_services(
		services.clone(),
		crate::host_core::ConnectionAdapters::from_services(&services),
		authority.clone(),
		authority,
		ProductContext::new(ProductId::for_test(product_id)),
	)
}

/// Whether this host keeps an AutoSigning key for `product_id` in this activation.
fn keeps_auto_signing(authority: &SigningHostRole, product_id: &str) -> bool {
	let session = authority.session_state().current().expect("an active wallet");
	futures::executor::block_on(
		authority.grants.auto_signing_key(&session, &ProductId::for_test(product_id)),
	)
	.expect("kept keys are readable")
	.is_some()
}

fn vrf_request(product_id: &str) -> v01::HostAccountSignVrfRequest {
	v01::HostAccountSignVrfRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test(product_id),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		transcript_label: b"pop:autosigning".to_vec(),
		items: vec![v01::VrfTranscriptItem { label: b"round".to_vec(), value: vec![1] }],
	}
}

fn full_person_key_handle() -> v01::ProductAccountId {
	v01::ProductAccountId {
		dot_ns_identifier: ProductId::for_test("peopl.dot"),
		derivation_index: v01::DerivationIndex::Index(0),
	}
}

fn full_person_ring_resolver() -> Arc<StubRingResolver> {
	let full_entropy = derive_ring_vrf_entropy(
		&ENTROPY,
		&ProductId::for_test("peopl.dot"),
		&v01::DerivationIndex::Index(0),
	)
	.expect("full-person entropy");
	let full_member = futures::executor::block_on(crate::runtime::vrf::load())
		.expect("verifiable is linked")
		.member(&full_entropy)
		.expect("full-person member");
	Arc::new(StubRingResolver {
		validation_gate: Default::default(),
		collection: *b"pop:polkadot.network/people     ",
		ring: ResolvedRing {
			selected: MemberCandidate { member: full_member },
			ring_index: 7,
			ring_revision: 11,
			domain_size: crate::runtime::vrf::DOMAIN_2E11,
			members: vec![full_member],
		},
	})
}

/// Seed `owner`'s cached manifest so a grant lookup resolves without a
/// chain. Mirrors `runtime::tests::cache_manifest`.
fn cache_grant(platform: &StubPlatform, owner: &str, trusted_products: &str) {
	let json = format!(
		r#"{{"$v":1,"displayName":"D","description":"d",
             "icon":{{"cid":"c","format":"png"}},"trustedProducts":{trusted_products}}}"#
	);
	let entry = crate::runtime::product_manifest::CachedManifest {
		fetched_at_secs: crate::unix_time::current_unix_secs(),
		json: Some(json),
	};
	futures::executor::block_on(
		<StubPlatform as crate::platform::CoreStorage>::write_core_storage(
			platform,
			test_storage_key(CoreStorageKey::product_manifest(&ProductId::for_test(owner))),
			parity_scale_codec::Encode::encode(&entry),
		),
	)
	.expect("stub core storage accepts the entry");
}

/// Persist a user refusal of `caller`'s access to `target`'s account.
fn deny_account_access(platform: &StubPlatform, caller: &str, target: &str) {
	futures::executor::block_on(
		// Bare-labelled on both sides, as `ProductConsent::account_access`
		// writes it in production.
		crate::host_internal::permissions::set_authorization_status(
			&test_storage(platform),
			CoreStorageKey::product_account_access_authorization(
				&ProductId::for_test(caller),
				&ProductId::for_test(target),
			),
			crate::platform::PermissionAuthorizationStatus::Denied,
		),
	)
	.expect("stub core storage accepts the decision");
}

/// A `context` grant lets a foreign product prove with the owner's key.
///
/// The test whose absence let the inert scope ship. The earlier
/// `a_cached_context_grant_lets_a_foreign_proof_through` asserted
/// `Rejected` with no session, which only proved the call reached the
/// session guard, and the authority one layer down would have refused it
/// anyway. This one runs the whole stack with a live session and a
/// registered key, so a proof actually comes back.
#[test]
fn a_context_grant_lets_a_foreign_product_prove_with_the_owners_key() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
	let (services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	let host = product_runtime_for(services, authority.clone(), "dim2.dot");
	let proof = futures::executor::block_on(
		host.create_account_proof(&CallContext::default(), foreign_proof_request(&ring_location)),
	);
	assert!(proof.is_ok(), "a granted cross-product proof must succeed, got {proof:?}");
}

/// The same call with no grant. Same fixture, one line different.
#[test]
fn a_foreign_proof_is_refused_when_the_owner_granted_nothing() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"someone-else":["context"]}"#);
	let (services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	let host = product_runtime_for(services, authority.clone(), "dim2.dot");
	let proof = futures::executor::block_on(
		host.create_account_proof(&CallContext::default(), foreign_proof_request(&ring_location)),
	);
	assert_eq!(
		proof.err(),
		Some(CallError::Domain(truapi::versioned::account::HostAccountCreateProofError::V1(
			v01::HostAccountCreateProofError::NotAllowlisted
		)))
	);
}

/// A refusal reaches the host log with what it was judged by.
///
/// The wire answers `NotAllowlisted` for every reason, so one device
/// refusing a grant every other device admits is only explicable from this
/// line: how old its cached copy was, or that it cached a miss, and what
/// the manifest lists for the caller.
///
/// The publisher can list any number of scopes, so a long list is cut by
/// the host sink after the source rather than pushing the source out.
#[test]
fn a_refusal_tells_the_host_what_it_judged_by() {
	let many_scopes = format!(r#"{{"dim2":[{}]}}"#, [r#""storage""#; 60].join(","));
	let cases = [
		(Some(r#"{"someone-else":["context"]}"#), "not granted", "None".to_string()),
		(Some(r#"{"dim2":["storage"]}"#), "not granted", "Some([Storage])".to_string()),
		(
			Some(many_scopes.as_str()),
			"not granted",
			format!("Some([{}])", ["Storage"; 60].join(", ")),
		),
		(None, "manifest unavailable", "None".to_string()),
	];
	let (judged, expected): (Vec<_>, Vec<_>) = cases
		.into_iter()
		.map(|(trusted_products, because, scopes)| {
			(
				refusal_lines_from_an_hour_old_cache(trusted_products),
				vec![refusal_line(&format!(
					"because={because}, entry=dim2, caller=dim2.dot, owner=peopl.dot, \
					 source=cache, 3600 s old, scopes={scopes}"
				))],
			)
		})
		.unzip();
	assert_eq!(judged, expected);
}

/// A lookup that read no manifest names the failure in the refusal, so it
/// is not mistaken for a product that publishes none.
#[test]
fn a_failed_lookup_is_named_in_the_refusal() {
	let lines = crate::logging::testing::host_sink_lines(|| {
		let _ = foreign_proof_through_the_authority(None);
	});
	assert_eq!(
		lines,
		vec![
			"truapi.tracing.warn truapi::runtime::product_manifest: root manifest lookup \
			 failed {target=peopl.dot, reason=Asset Hub follow initialization timed out}"
				.to_string(),
			refusal_line(
				"because=manifest unavailable, entry=dim2, caller=dim2.dot, \
				 owner=peopl.dot, source=dotNS lookup failed: Asset Hub follow \
				 initialization timed out, scopes=None"
			),
		]
	);
}

/// A refused signature reaches the host log as a refused proof does.
#[test]
fn a_refused_signature_tells_the_host_what_it_judged_by() {
	let seeded_at_secs = crate::unix_time::current_unix_secs();
	let lines = crate::logging::testing::host_sink_lines(|| {
		let _ =
			foreign_ring_vrf_sign_through_the_authority(Some(r#"{"someone-else":["context"]}"#));
	});
	assert_eq!(
		pin_cache_ages(lines, seeded_at_secs, 0),
		vec![refusal_line(
			"because=not granted, entry=dim2, caller=dim2.dot, owner=peopl.dot, \
			 source=cache, 0 s old, scopes=None"
		)]
	);
}

/// A refusal by the context check names the context it refused.
///
/// A stored denial lists the scopes the manifest grants, which shows the
/// manifest was not the reason. A context on another network reads no
/// manifest at all, so its line has no source.
#[test]
fn a_context_refusal_tells_the_host_which_context() {
	let context_grants = Some(r#"{"dim2next":["context"]}"#);
	let seeded_at_secs = crate::unix_time::current_unix_secs();
	let denied = crate::logging::testing::host_sink_lines(|| {
		let _ = third_product_context(
			OWNER_GRANTS_DIM2NEXT,
			context_grants,
			context_of("dim2.dot"),
			|platform| deny_account_access(platform, "dim2next.dot", "dim2.dot"),
		);
	});
	let off_network = crate::logging::testing::host_sink_lines(|| {
		let _ = third_product_context(
			OWNER_GRANTS_DIM2NEXT,
			context_grants,
			context_of("dim2.paseo"),
			|_| {},
		);
	});
	let denied_line = refusal_line(
		"because=user denied, entry=dim2next, caller=dim2next.dot, owner=peopl.dot, \
		 context=dim2.dot, source=cache, 0 s old, scopes=Some([Context])",
	);
	let off_network_line = refusal_line(
		"because=context on another network, entry=dim2next, caller=dim2next.dot, \
		 owner=peopl.dot, context=dim2.paseo, scopes=None",
	);
	assert_eq!(
		(pin_cache_ages(denied, seeded_at_secs, 0), off_network),
		(vec![denied_line.clone(), denied_line], vec![off_network_line.clone(), off_network_line]),
		"one line for the proof and one for the alias read"
	);
}

/// A granted proof and alias read stay off the host log.
#[test]
fn a_granted_proof_warns_nobody() {
	let lines = crate::logging::testing::host_sink_lines(|| {
		let (proof, alias) = third_product_context(
			OWNER_GRANTS_DIM2NEXT,
			Some(r#"{"dim2next":["context"]}"#),
			context_of("dim2.dot"),
			|_| {},
		);
		assert!(proof.is_ok() && alias.is_ok(), "both are granted: {proof:?}, {alias:?}");
	});
	assert_eq!(lines, Vec::<String>::new());
}

/// An alias read the owner did not grant falls back to the consent the user
/// already gave, so the key-grant refusal on the way is not a refusal.
#[test]
fn a_consented_alias_read_warns_nobody() {
	let platform =
		Arc::new(StubPlatform { account_access_confirmed: true, ..StubPlatform::default() });
	// Cached, so no failed chain lookup warns on the way.
	cache_grant(&platform, "peopl.dot", r#"{"someone-else":["context"]}"#);
	let authority =
		SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	let lines = crate::logging::testing::host_sink_lines(|| {
		futures::executor::block_on(authority.account_alias(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test("myapp.dot"),
				payload: HostAccountGetAliasRequest {
					key_handle: full_person_key_handle(),
					context: context_of("other.dot"),
					ring_location,
				},
			},
		))
		.expect("the user consented to the read");
	});
	assert_eq!(lines, Vec::<String>::new());
}

/// The host log line of a ring-VRF refusal carrying `fields`, cut where the
/// host sink cuts it.
fn refusal_line(fields: &str) -> String {
	let detail: String =
		format!("truapi::runtime::product_manifest: ring-VRF access refused {{{fields}}}")
			.chars()
			.take(crate::logging::HOST_SINK_DETAIL_MAX_CHARS)
			.collect();
	format!("truapi.tracing.warn {detail}")
}

/// `lines` with each cache age a lookup since `seeded_at_secs` can report
/// written as `age_at_seeding`, the age the entry had when it was seeded.
fn pin_cache_ages(lines: Vec<String>, seeded_at_secs: u64, age_at_seeding: u64) -> Vec<String> {
	// The clock may tick between seeding and the lookup.
	let elapsed = crate::unix_time::current_unix_secs() - seeded_at_secs;
	let ages = age_at_seeding..=age_at_seeding + elapsed;
	lines
		.into_iter()
		.map(|line| {
			ages.clone().fold(line, |line, age| {
				line.replace(
					&format!("cache, {age} s old"),
					&format!("cache, {age_at_seeding} s old"),
				)
			})
		})
		.collect()
}

/// The host log lines of `dim2.dot` asking for the key of `peopl.dot`, with
/// the manifest of `peopl.dot` cached an hour ago listing `trusted_products`,
/// or `None` for a cached answer that it publishes no manifest.
fn refusal_lines_from_an_hour_old_cache(trusted_products: Option<&str>) -> Vec<String> {
	const HOUR_SECS: u64 = 60 * 60;
	let seeded_at_secs = crate::unix_time::current_unix_secs();
	let fetched_at_secs = seeded_at_secs - HOUR_SECS;
	let json = trusted_products.map(crate::host_internal::product_manifest::test_manifest_json);
	let lines = crate::logging::testing::host_sink_lines(|| {
		let _ = foreign_proof_through_the_authority_with(None, |platform| {
			futures::executor::block_on(
				<StubPlatform as crate::platform::CoreStorage>::write_core_storage(
					platform,
					test_storage_key(CoreStorageKey::product_manifest(&ProductId::for_test(
						"peopl.dot",
					))),
					crate::runtime::product_manifest::encode_cached_root_manifest(
						json.as_deref(),
						fetched_at_secs,
					),
				),
			)
			.expect("stub core storage accepts the entry");
		});
	});
	pin_cache_ages(lines, seeded_at_secs, HOUR_SECS)
}

/// **The one that matters.** Nothing in the request can stand in for the
/// manifest.
///
/// This drives the authority directly, the way `sso_responder` does for a
/// request arriving over the pairing wire. The frontend, and its grant
/// check, are not on this path at all. The request names `dim2.dot` as the
/// caller and `peopl.dot`'s key as the handle, which is the most a peer can
/// assert. With no grant published it is refused; with the grant published
/// and nothing else changed it succeeds. So the admitting fact is the
/// manifest the authority resolved for itself, not any field the caller
/// set.
///
/// What makes this cover the wire path is an invariant, not a convention:
/// the authority has exactly one behaviour, shared by both doors, because
/// nothing it reads records which door a request came through. Should a
/// door-dependent relaxation ever land (a dev allowlist consulted only for
/// local callers, say), that invariant is gone, and this test has to
/// declare the paired-peer door explicitly or it silently stops covering
/// it. Whoever adds the distinction owns updating this.
#[test]
fn a_request_cannot_substitute_for_the_owners_manifest() {
	let refusal = foreign_proof_through_the_authority(None);
	assert_eq!(
		refusal.err(),
		Some(RingVrfError::NotAllowlisted),
		"with no manifest grant the authority must refuse, whatever the request says"
	);

	let granted = foreign_proof_through_the_authority(Some(r#"{"dim2":["context"]}"#));
	assert!(
		granted.is_ok(),
		"the identical request must succeed once the owner's manifest grants it, \
         which is what proves the manifest is the deciding input; got {granted:?}"
	);
}

#[test]
fn a_blessed_product_proves_with_a_context_grant_despite_a_stored_denial() {
	let proof =
		foreign_proof_through_the_authority_with(Some(r#"{"dim2":["context"]}"#), |platform| {
			deny_account_access(platform, "dim2.dot", "peopl.dot")
		});
	assert!(proof.is_ok(), "blessed account access ignores the saved denial: {proof:?}");
}

/// A `context` grant covers the identity read, so the two calls agree.
///
/// The contextual alias and the proof come out of one VRF evaluation, so a
/// grantee that may `create_proof` already holds the alias that proof
/// attests. Gating `account_alias` on a prompt while `create_proof` is
/// gated on the grant left the same bytes reachable one way and refused the
/// other, which is not a policy anyone chose. RFC-0024 defines `context` as
/// reading the account "and the identity that follows from it".
///
/// An ordinary product with no grant still takes the prompt, and a stored
/// refusal still overrides its grant.
#[test]
fn a_context_grant_covers_the_identity_read() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);
	let context = v01::ProductProofContext {
		product_id: ProductId::for_test("dim2.dot"),
		suffix: v01::DerivationIndex::Index(0),
	};
	let alias_for = |caller: &str| {
		futures::executor::block_on(authority.account_alias(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test(caller),
				payload: v01::HostAccountGetAliasRequest {
					key_handle: full_person_key_handle(),
					context: context.clone(),
					ring_location: ring.clone(),
				},
			},
		))
	};

	let owner = alias_for("peopl.dot").expect("the owner reads its own alias");
	let granted = alias_for("dim2.dot").expect("a context grant covers the identity read");
	assert_eq!(
		granted.alias, owner.alias,
		"the grantee must read the owner's alias, not one derived for itself"
	);
	assert_eq!(
		platform
			.account_access_reviews
			.lock()
			.expect("review list mutex poisoned")
			.len(),
		0,
		"a granted read must not raise the prompt the grant already answers"
	);
	assert_eq!(
		alias_for("ordinary.dot").err(),
		Some(RingVrfError::Rejected),
		"a product with no grant still takes the prompt path and is refused"
	);
}

/// A grant does not let the grantee choose whose pseudonym to mint.
///
/// The contextual alias is a function of (owner key, context), so with the
/// context unconstrained a `context` grant from `peopl.dot` let `dim2.dot`
/// produce the alias `peopl.dot` presents to `bank.dot`, a third product
/// that granted nothing and is not a party to the grant. The grant is to act
/// in the grantee's own context or the granting product's: a context naming
/// the owner is the grant read literally, and is the one a chain-wide proof
/// context resolves to. Any other product's context needs that product's own
/// grant, covered by `a_product_may_grant_a_grantee_its_context`.
///
/// The owner's own calls are untouched: minting your own aliases in any
/// context is what the context parameter is for.
#[test]
fn a_grantee_cannot_mint_the_owners_alias_in_a_third_partys_context() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);

	let mint = |caller: &str, context: &str| {
		futures::executor::block_on(authority.create_proof(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test(caller),
				payload: v01::HostAccountCreateProofRequest {
					key_handle: full_person_key_handle(),
					context: v01::ProductProofContext {
						product_id: ProductId::for_test(context),
						suffix: v01::DerivationIndex::Index(0),
					},
					ring_location: ring.clone(),
					message: b"m".to_vec(),
				},
			},
		))
	};

	assert_eq!(
		mint("dim2.dot", "bank.dot").err(),
		Some(RingVrfError::NotAllowlisted),
		"the grantee must not mint the owner's pseudonym for a third product"
	);
	assert!(
		mint("dim2.dot", "dim2.dot").is_ok(),
		"the grant admits the grantee acting in its own context"
	);
	assert!(
		mint("dim2.dot", "peopl.dot").is_ok(),
		"the grant admits the grantee acting in the granting product's context"
	);
	assert!(
		mint("peopl.dot", "bank.dot").is_ok(),
		"the owner may still mint its own alias in any context"
	);
	assert!(
		mint("dim2.dot", "app.peopl.dot").is_ok(),
		"the granting product is all its executables, so its context is too"
	);
	assert_eq!(
		mint("dim2.dot", "dim2.paseo").err(),
		Some(RingVrfError::NotAllowlisted),
		"the grantee's own namesake on another network is a different \
         product, so its context is not the grantee's"
	);
	assert_eq!(
		mint("dim2.dot", "peopl.paseo").err(),
		Some(RingVrfError::NotAllowlisted),
		"a grant published on one network must not reach the pseudonym a \
         namesake presents on another"
	);
}

/// `dim2next.dot`'s proof and alias read with `peopl.dot`'s key in
/// `context`, after seeding `peopl.dot`'s manifest with `owner_grants` and
/// `dim2`'s with `context_grants`.
///
/// `context_grants` of `None` caches the chain's answer that `dim2`
/// publishes no manifest, so no case reaches for a chain the stub lacks.
fn third_product_context(
	owner_grants: &str,
	context_grants: Option<&str>,
	context: v01::ProductProofContext,
	seed: impl FnOnce(&StubPlatform),
) -> (
	Result<v01::HostAccountCreateProofResponse, RingVrfError>,
	Result<v01::ContextualAlias, RingVrfError>,
) {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", owner_grants);
	match context_grants {
		Some(grants) => cache_grant(&platform, "dim2.dot", grants),
		None => futures::executor::block_on(
			<StubPlatform as crate::platform::CoreStorage>::write_core_storage(
				&platform,
				test_storage_key(CoreStorageKey::product_manifest(&ProductId::for_test(
					"dim2.dot",
				))),
				crate::runtime::product_manifest::encode_cached_root_manifest(
					None,
					crate::unix_time::current_unix_secs(),
				),
			),
		)
		.expect("stub core storage accepts the entry"),
	}
	seed(&platform);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);

	let proof = futures::executor::block_on(authority.create_proof(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("dim2next.dot"),
			payload: v01::HostAccountCreateProofRequest {
				key_handle: full_person_key_handle(),
				context: context.clone(),
				ring_location: ring.clone(),
				message: b"m".to_vec(),
			},
		},
	));
	let alias = futures::executor::block_on(authority.account_alias(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("dim2next.dot"),
			payload: v01::HostAccountGetAliasRequest {
				key_handle: full_person_key_handle(),
				context,
				ring_location: ring,
			},
		},
	));
	(proof, alias)
}

fn context_of(product_id: &str) -> v01::ProductProofContext {
	v01::ProductProofContext {
		product_id: ProductId::for_test(product_id),
		suffix: v01::DerivationIndex::Index(0),
	}
}

const OWNER_GRANTS_DIM2NEXT: &str = r#"{"dim2next":["context"]}"#;

/// The product a context names consents to its use through its own manifest.
///
/// paritytech/platform-bugs#99: `dim2next` proves with `peopl`'s key in
/// `dim2`'s context, which the TURN issuer requires. `peopl` grants the key
/// and `dim2` grants the context, so both parties to the pseudonym have
/// consented. The read is held to the same rule, since the alias and the
/// proof are one VRF evaluation.
#[test]
fn a_product_may_grant_a_grantee_its_context() {
	for (grants, context) in [
		(r#"{"dim2next":["context"]}"#, "dim2.dot"),
		(r#"{"dim2next":["all"]}"#, "dim2.dot"),
		(r#"{"dim2next":["context"]}"#, "app.dim2.dot"),
		(r#"{"dim2next":["context"]}"#, "DIM2.DOT"),
	] {
		let (proof, alias) =
			third_product_context(OWNER_GRANTS_DIM2NEXT, Some(grants), context_of(context), |_| {});
		assert!(proof.is_ok(), "{grants} admits a proof in {context}: {proof:?}");
		assert!(alias.is_ok(), "{grants} admits the read in {context}: {alias:?}");
	}
}

/// Everything a context grant does not cover stays refused, on both calls.
#[test]
fn a_context_the_naming_product_did_not_grant_is_refused() {
	let cases: [(&str, Option<&str>, &str, &str); 5] = [
		(
			OWNER_GRANTS_DIM2NEXT,
			None,
			"dim2.dot",
			"a product publishing no manifest granted nothing",
		),
		(
			OWNER_GRANTS_DIM2NEXT,
			Some(r#"{}"#),
			"dim2.dot",
			"a manifest naming no one granted nothing",
		),
		(
			OWNER_GRANTS_DIM2NEXT,
			Some(r#"{"other":["context"]}"#),
			"dim2.dot",
			"a grant to another product is not a grant to the caller",
		),
		(
			OWNER_GRANTS_DIM2NEXT,
			Some(r#"{"dim2next":["storage"]}"#),
			"dim2.dot",
			"a narrower scope does not imply context",
		),
		(
			// `dim2.dot`'s manifest is cached by bare label, so it is the
			// document a `.paseo` context would read: a namesake's grant.
			OWNER_GRANTS_DIM2NEXT,
			Some(r#"{"dim2next":["context"]}"#),
			"dim2.paseo",
			"a grant must not reach the pseudonym on another network",
		),
	];
	for (owner, grants, context, why) in cases {
		let (proof, alias) = third_product_context(owner, grants, context_of(context), |_| {});
		assert_eq!(proof.err(), Some(RingVrfError::NotAllowlisted), "proof: {why}");
		assert_eq!(alias.err(), Some(RingVrfError::NotAllowlisted), "read: {why}");
	}
}

/// A context grant does not open the owner's key.
///
/// The two grants answer different questions: `dim2` consents to its
/// pseudonym, `peopl` to its key. Without the owner's grant the gate refuses
/// before the context is looked at, and the read falls back to the prompt.
#[test]
fn a_context_grant_without_the_owners_key_grant_is_refused() {
	let (proof, alias) = third_product_context(
		r#"{"someone-else":["context"]}"#,
		Some(r#"{"dim2next":["context"]}"#),
		context_of("dim2.dot"),
		|_| {},
	);
	assert_eq!(proof.err(), Some(RingVrfError::NotAllowlisted));
	assert_eq!(
		alias.err(),
		Some(RingVrfError::Rejected),
		"the ungranted read takes the prompt path, which the stub declines"
	);
}

/// A refusal the user stored for the pair overrides the context product's
/// grant, as it overrides the owner's.
#[test]
fn a_stored_refusal_overrides_a_context_grant() {
	let (proof, alias) = third_product_context(
		OWNER_GRANTS_DIM2NEXT,
		Some(r#"{"dim2next":["context"]}"#),
		context_of("dim2.dot"),
		|platform| deny_account_access(platform, "dim2next.dot", "dim2.dot"),
	);
	assert_eq!(proof.err(), Some(RingVrfError::NotAllowlisted));
	assert_eq!(alias.err(), Some(RingVrfError::NotAllowlisted));
}

/// The caller's own context and the owner's need no third manifest.
#[test]
fn own_and_owner_contexts_need_no_context_grant() {
	for context in ["dim2next.dot", "peopl.dot"] {
		let (proof, alias) =
			third_product_context(OWNER_GRANTS_DIM2NEXT, None, context_of(context), |_| {});
		assert!(proof.is_ok(), "proof in {context}: {proof:?}");
		assert!(alias.is_ok(), "read in {context}: {alias:?}");
	}
}

/// An owner listing its own keys is not asked to consent to its own account.
///
/// `sso_responder` hands `calling_product_id` through untouched, so
/// comparing it raw against the normalized owner makes an owner that spells
/// itself differently look like a stranger: it is prompted, and the decision
/// is filed under the spelling the peer chose rather than the one the grant
/// path reads back.
#[test]
fn an_owner_listing_its_own_keys_is_not_prompted_for_its_own_account() {
	let platform = Arc::new(StubPlatform::default());
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);

	let listed = futures::executor::block_on(authority.list_ring_vrf_keys(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("PEOPL.DOT"),
			payload: v01::HostAccountListRingVrfKeysRequest {
				owner: ProductId::for_test("peopl.dot"),
				disclosure: v01::RingVrfKeyDisclosure::PublicKey,
			},
		},
	));
	assert!(
		listed.is_ok(),
		"an owner must reach its own keys however it spells itself; got {:?}",
		listed.err()
	);
	assert_eq!(
		platform
			.account_access_reviews
			.lock()
			.expect("review list mutex poisoned")
			.len(),
		0,
		"and must not be asked to consent to its own account"
	);
}

/// The context guard answers "is this the owner?" the way the gate does.
///
/// The gate short-circuits on full normalized ids; comparing bare labels in
/// the guard made a caller `peopl.paseo` against a handle `peopl.dot` a
/// stranger to one and the owner to the other, so the grant was demanded and
/// the context then left unconstrained. Two functions answering one question
/// differently is what this change otherwise collapses.
#[test]
fn a_cross_network_namesake_is_not_treated_as_the_owner() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"peopl":["context"]}"#);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);

	// `peopl.paseo` shares a label with the key's owner `peopl.dot` but is a
	// different product, so the grant admits it and the context still binds.
	let minted = futures::executor::block_on(authority.create_proof(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("peopl.paseo"),
			payload: v01::HostAccountCreateProofRequest {
				key_handle: full_person_key_handle(),
				context: v01::ProductProofContext {
					product_id: ProductId::for_test("bank.dot"),
					suffix: v01::DerivationIndex::Index(0),
				},
				ring_location: ring.clone(),
				message: b"m".to_vec(),
			},
		},
	));
	assert_eq!(
		minted.err(),
		Some(RingVrfError::NotAllowlisted),
		"a namesake on another network is not the owner, so its context binds"
	);
}

/// A grantee spelling its own context differently is still in its own context.
///
/// `context.product_id` arrives straight off the request payload. Left raw it
/// was the one identity the gate had not normalized, so a grantee naming its
/// context `DIM2.DOT` was refused for the spelling rather than the scope.
#[test]
fn a_grantee_may_spell_its_own_context_differently() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);
	let mint = |context: &str| {
		futures::executor::block_on(authority.create_proof(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test("dim2.dot"),
				payload: v01::HostAccountCreateProofRequest {
					key_handle: full_person_key_handle(),
					context: v01::ProductProofContext {
						product_id: ProductId::for_test(context),
						suffix: v01::DerivationIndex::Index(0),
					},
					ring_location: ring.clone(),
					message: b"m".to_vec(),
				},
			},
		))
	};
	assert!(mint("dim2.dot").is_ok(), "control: the canonical spelling");
	assert!(
		mint("DIM2.DOT").is_ok(),
		"the same context in another spelling is still the caller's own"
	);
	assert_eq!(
		mint("bank.dot").err(),
		Some(RingVrfError::NotAllowlisted),
		"and a third party's context is still refused"
	);
}

/// A refusal recorded by an earlier release still overrides a grant.
///
/// This release files the decision under the product label; earlier ones
/// used the full product id. Reading only the new shape would discard the
/// old decision, and on the granted path it would not even re-ask, because
/// the lookup reads `NotDetermined`, admits the grant and raises no prompt.
/// A user's "no" would become a "yes" on upgrade.
#[test]
fn a_refusal_recorded_before_this_release_still_overrides_a_grant() {
	use crate::host_internal::product_manifest::Granted;
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
	// Written exactly as the previous release wrote it: full ids, both sides.
	futures::executor::block_on(crate::host_internal::permissions::set_authorization_status(
		&test_storage(platform.as_ref()),
		CoreStorageKey::account_access_authorization(
			&ProductId::for_test("ordinary.dot"),
			&ProductId::for_test("peopl.dot"),
		),
		crate::platform::PermissionAuthorizationStatus::Denied,
	))
	.expect("stub core storage accepts the decision");
	let (services, _authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

	assert!(
		!futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
			&services,
			platform.as_ref(),
			&ProductId::for_test("ordinary.dot"),
			&ProductId::for_test("peopl.dot"),
			Granted::Context,
		)),
		"a decision stored under the previous key shape must still be honoured"
	);
	assert_eq!(
		platform
			.account_access_reviews
			.lock()
			.expect("review list mutex poisoned")
			.len(),
		0,
		"and it must be honoured without re-asking"
	);
}

/// A refusal covers the refused product's subnames on the grantor side too.
///
/// The grant is resolved by the target's bare label, so filing the refusal
/// against the full target let `app.peopl.dot` carry a grant the user had
/// refused for `peopl.dot`. The argument the PR makes for the requester
/// applies unchanged to the grantor.
#[test]
fn a_refusal_covers_every_executable_of_the_refused_target() {
	use crate::host_internal::product_manifest::Granted;
	let platform = Arc::new(StubPlatform {
		// The user declines, so the refusal is written by the production
		// path rather than by a test helper: this has to pin where
		// `ProductConsent::account_access` files it, not where a fixture does.
		account_access_confirmed: false,
		..StubPlatform::default()
	});
	cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
	futures::executor::block_on(
		crate::runtime::product_consent::ProductConsent::new(
			platform.clone(),
			crate::test_support::paseo_test_stack(),
		)
		.account_access(
			&ProductId::for_test("ordinary.dot"),
			&ProductId::for_test("peopl.dot"),
			platform.as_ref(),
		),
	)
	.expect("the stub records the declined decision");
	let (services, _authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	for target in ["peopl.dot", "app.peopl.dot", "worker.peopl.dot"] {
		assert!(
			!futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
				&services,
				platform.as_ref(),
				&ProductId::for_test("ordinary.dot"),
				&ProductId::for_test(target),
				Granted::Context,
			)),
			"{target} is the refused product wearing another name"
		);
	}
}

/// The identity read is held to the same contexts as the proof.
///
/// The alias and the proof come out of one VRF evaluation, so a guard on
/// `create_proof` alone leaves the same bytes reachable through
/// `account_alias`: a grantee could read the alias the owner presents to a
/// third product that granted nothing. Both calls refuse it, and both admit
/// the granting product's own context. A third product's own grant is
/// covered by `a_product_may_grant_a_grantee_its_context`.
#[test]
fn a_grantee_cannot_read_the_owners_alias_in_a_third_partys_context() {
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"dim2":["context"]}"#);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);
	let alias = |caller: &str, context: &str| {
		futures::executor::block_on(authority.account_alias(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test(caller),
				payload: v01::HostAccountGetAliasRequest {
					key_handle: full_person_key_handle(),
					context: v01::ProductProofContext {
						product_id: ProductId::for_test(context),
						suffix: v01::DerivationIndex::Index(0),
					},
					ring_location: ring.clone(),
				},
			},
		))
	};

	assert_eq!(
		alias("dim2.dot", "bank.dot").err(),
		Some(RingVrfError::NotAllowlisted),
		"a grantee must not read the owner's pseudonym for a third product"
	);
	assert!(alias("dim2.dot", "dim2.dot").is_ok(), "the grant covers the grantee's own context");
	assert!(
		alias("dim2.dot", "peopl.dot").is_ok(),
		"the grant covers the granting product's own context"
	);
	assert!(
		alias("dim2.dot", "app.peopl.dot").is_ok(),
		"the granting product is all its executables here too"
	);
	assert_eq!(
		alias("dim2.dot", "peopl.paseo").err(),
		Some(RingVrfError::NotAllowlisted),
		"the network rule binds the read as well as the proof: the alias and \
         the proof come out of one VRF evaluation"
	);
	assert!(
		alias("peopl.dot", "bank.dot").is_ok(),
		"the owner may still read its own alias in any context"
	);
}

/// An owner naming itself in another spelling is admitted, over the wire.
///
/// The context guard compares the identities the gate normalized, not the
/// ones the request carried. Deriving the caller from the request again
/// compares a peer's spelling against a normalized owner and refuses the
/// owner on its own key. `require_ring_vrf_key_access` returns the
/// normalized owner to stop exactly that, and `sso_responder` hands
/// `calling_product_id` through untouched.
#[test]
fn an_owner_spelled_differently_still_proves_with_its_own_key() {
	let platform = Arc::new(StubPlatform::default());
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring);
	let prove = |caller: &str| {
		futures::executor::block_on(authority.create_proof(
			&CallContext::default(),
			&session,
			ProductRequest {
				calling_product_id: ProductId::for_test(caller),
				payload: v01::HostAccountCreateProofRequest {
					key_handle: full_person_key_handle(),
					context: v01::ProductProofContext {
						product_id: ProductId::for_test("peopl.dot"),
						suffix: v01::DerivationIndex::Index(0),
					},
					ring_location: ring.clone(),
					message: b"m".to_vec(),
				},
			},
		))
	};
	assert!(prove("peopl.dot").is_ok(), "control: the canonical spelling");
	assert!(
		prove("PEOPL.DOT").is_ok(),
		"the owner must not be refused on its own key for spelling itself differently"
	);
}

/// A refusal covers the product, not one spelling of it.
///
/// The manifest grants a product and a product is all its executables, so a
/// refusal that overrides the grant has to name the same party. Keyed by the
/// full id instead, the user refusing `dim2.dot` left `app.dim2.dot` holding
/// the identical grant: the product re-entered under a subname it already
/// owns and the override was gone. Both halves now key by the bare label.
#[test]
fn a_refusal_covers_every_executable_of_the_refused_product() {
	use crate::host_internal::product_manifest::Granted;
	let platform = Arc::new(StubPlatform::default());
	cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
	deny_account_access(&platform, "ordinary.dot", "peopl.dot");
	let (services, _authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

	let granted = |caller: &str| {
		futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
			&services,
			platform.as_ref(),
			&ProductId::for_test(caller),
			&ProductId::for_test("peopl.dot"),
			Granted::Context,
		))
	};
	for spelling in ["ordinary.dot", "app.ordinary.dot", "worker.ordinary.dot", "ordinary.paseo"] {
		assert!(!granted(spelling), "{spelling} is the refused product wearing another name");
	}

	// Control: a product the user never refused still holds its own grant,
	// so the assertions above are not passing because nothing is granted.
	let clean = Arc::new(StubPlatform::default());
	cache_grant(&clean, "peopl.dot", r#"{"ordinary":["context"]}"#);
	let (services, _authority) =
		signing_runtime_with_ring_resolver(clean.clone(), full_person_ring_resolver());
	assert!(
		futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
			&services,
			clean.as_ref(),
			&ProductId::for_test("app.ordinary.dot"),
			&ProductId::for_test("peopl.dot"),
			Granted::Context,
		)),
		"control: with no refusal recorded the same subname is granted"
	);
}

/// `all` is a superset, so it satisfies `context` at the runtime seam and
/// not only in the manifest parser.
#[test]
fn a_grant_of_all_satisfies_context_at_the_authority() {
	let granted = foreign_proof_through_the_authority(Some(r#"{"dim2":["all"]}"#));
	assert!(granted.is_ok(), "`all` must satisfy `context`, got {granted:?}");
}

fn signing_runtime_with_ring_resolver(
	platform: Arc<StubPlatform>,
	ring_resolver: Arc<StubRingResolver>,
) -> (Arc<RuntimeServices>, Arc<SigningHostRole>) {
	let authority = SigningHostRole::new_with_ring_resolver(platform, ring_resolver);
	(authority.services.clone(), authority)
}

/// The grant admits `ring_vrf_sign`, not only `create_proof`.
///
/// #655 lists this as untested and it was: the other grant tests here all
/// drive `create_proof`. Both authorities call the same
/// `require_ring_vrf_key_access` from both methods, so the code was
/// covered, but a scope that admits one call and not the other is exactly
/// the kind of half-wired gate this issue exists to fix, and nothing
/// asserted the second half.
///
/// Driven at the authority, where the wire path also arrives, so this
/// covers the paired case as well. Success is the owner's own signature:
/// the grant lets `dim2.dot` produce what `peopl.dot` would have.
#[test]
fn a_context_grant_lets_a_foreign_product_sign_with_the_owners_key() {
	let granted = foreign_ring_vrf_sign_through_the_authority(Some(r#"{"dim2":["context"]}"#));
	let owners_own = foreign_ring_vrf_sign_through_the_authority_as(
		Some(r#"{"dim2":["context"]}"#),
		"peopl.dot",
	);
	assert!(
		granted.is_ok(),
		"a granted cross-product ring-VRF signature must be produced, got {granted:?}"
	);
	assert_eq!(
		granted, owners_own,
		"the grant must yield the owner's own signature, not a caller-derived one"
	);
}

/// The same call with no grant.
#[test]
fn a_foreign_ring_vrf_sign_is_refused_when_the_owner_granted_nothing() {
	assert_eq!(
		foreign_ring_vrf_sign_through_the_authority(None).err(),
		Some(RingVrfError::NotAllowlisted)
	);
}

/// An unreadable permission store refuses an ordinary product's grant.
///
/// The stored `AccountAccess` decision is the only thing that can override a
/// publisher's grant. Reading a storage fault as "not refused" would let a
/// locked keychain turn the user's explicit no into a yes, on the strength
/// of a manifest the publisher controls. `ProductConsent::account_access`,
/// which writes that same decision, already fails closed; this is the read
/// side agreeing with it.
///
/// The error is scoped to permission keys so the manifest cache still
/// answers: otherwise the call would refuse for want of a manifest and the
/// assertion would prove nothing.
#[test]
fn a_grant_is_refused_when_the_stored_decision_cannot_be_read() {
	let platform = Arc::new(StubPlatform {
		permission_storage_error: Some("keychain locked"),
		..StubPlatform::default()
	});
	cache_grant(&platform, "peopl.dot", r#"{"ordinary":["context"]}"#);
	let (services, _authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

	let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
		&services,
		platform.as_ref(),
		&ProductId::for_test("ordinary.dot"),
		&ProductId::for_test("peopl.dot"),
		crate::host_internal::product_manifest::Granted::Context,
	));
	assert!(
		!granted,
		"an unreadable permission store must refuse, not fall through to the manifest"
	);

	// Control: the identical grant, with the store readable, is honoured.
	// Without this the assertion above would also pass if the grant never
	// worked at all.
	let readable = Arc::new(StubPlatform::default());
	cache_grant(&readable, "peopl.dot", r#"{"ordinary":["context"]}"#);
	let (services, _authority) =
		signing_runtime_with_ring_resolver(readable.clone(), full_person_ring_resolver());
	assert!(
		futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
			&services,
			readable.as_ref(),
			&ProductId::for_test("ordinary.dot"),
			&ProductId::for_test("peopl.dot"),
			crate::host_internal::product_manifest::Granted::Context,
		)),
		"control: the same grant must be honoured when the store reads cleanly"
	);
}

/// A grant lookup with nothing cached reaches the chain, and dials the
/// Asset Hub the role was configured with.
///
/// Every other grant test here seeds the manifest cache, and `root_manifest`
/// serves that before it consults the genesis hash, so the whole suite
/// passes on a role with no Asset Hub, and deleting the production install
/// would not turn any of it red. That is the blind spot #660 survived in.
/// This is the one case that takes the other branch: it asserts the dial
/// itself, so removing the install breaks it rather than going unnoticed.
///
/// The stub answers no RPC, so the lookup fails closed and the call is
/// refused. What is pinned is that the chain was reached at all, and which
/// chain.
#[test]
fn a_grant_lookup_with_a_cold_cache_dials_the_configured_asset_hub() {
	let platform = Arc::new(StubPlatform::default());
	// Deliberately no `cache_grant`: this must take the chain branch.
	let (services, _authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());

	let granted = futures::executor::block_on(crate::runtime::product_manifest::grants_scope(
		&services,
		platform.as_ref(),
		&ProductId::for_test("dim2.dot"),
		&ProductId::for_test("peopl.dot"),
		crate::host_internal::product_manifest::Granted::Context,
	));
	assert!(!granted, "the stub answers no RPC, so the lookup must fail closed");

	let dialled = platform
		.chain_connects
		.lock()
		.expect("chain connect list mutex poisoned")
		.clone();
	assert!(
		dialled.contains(&[0xcc; 32]),
		"a cold-cache grant lookup must dial the configured Asset Hub; dialled {dialled:?}"
	);
}

fn foreign_ring_vrf_sign_through_the_authority(
	trusted_products: Option<&str>,
) -> Result<Vec<u8>, RingVrfError> {
	foreign_ring_vrf_sign_through_the_authority_as(trusted_products, "dim2.dot")
}

/// Drive `ring_vrf_sign` straight at the authority with `caller` naming
/// `peopl.dot`'s key handle, bypassing the frontend exactly as
/// `sso_responder` does.
fn foreign_ring_vrf_sign_through_the_authority_as(
	trusted_products: Option<&str>,
	caller: &str,
) -> Result<Vec<u8>, RingVrfError> {
	let platform = Arc::new(StubPlatform::default());
	if let Some(trusted_products) = trusted_products {
		cache_grant(&platform, "peopl.dot", trusted_products);
	}
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform, full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	register_full_person_key(&authority, &session, &full_person_ring_location());

	futures::executor::block_on(authority.ring_vrf_sign(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test(caller),
			payload: v01::HostAccountRingVrfSignRequest {
				key_handle: full_person_key_handle(),
				message: b"sign me".to_vec(),
			},
		},
	))
}

fn foreign_proof_through_the_authority(
	trusted_products: Option<&str>,
) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
	foreign_proof_through_the_authority_with(trusted_products, |_| {})
}

/// Drive `create_proof` straight at the authority, bypassing the frontend,
/// with `dim2.dot` naming `peopl.dot`'s key handle.
fn foreign_proof_through_the_authority_with(
	trusted_products: Option<&str>,
	seed: impl FnOnce(&StubPlatform),
) -> Result<v01::HostAccountCreateProofResponse, RingVrfError> {
	let platform = Arc::new(StubPlatform::default());
	if let Some(trusted_products) = trusted_products {
		cache_grant(&platform, "peopl.dot", trusted_products);
	}
	seed(&platform);
	let (_services, authority) =
		signing_runtime_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	futures::executor::block_on(authority.create_proof(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("dim2.dot"),
			payload: v01::HostAccountCreateProofRequest {
				key_handle: full_person_key_handle(),
				context: v01::ProductProofContext {
					product_id: ProductId::for_test("dim2.dot"),
					suffix: v01::DerivationIndex::Index(0),
				},
				ring_location,
				message: b"prove me".to_vec(),
			},
		},
	))
}

fn foreign_proof_request(
	ring_location: &v01::RingLocation,
) -> truapi::versioned::account::HostAccountCreateProofRequest {
	truapi::versioned::account::HostAccountCreateProofRequest::V1(
		v01::HostAccountCreateProofRequest {
			key_handle: full_person_key_handle(),
			context: v01::ProductProofContext {
				product_id: ProductId::for_test("dim2.dot"),
				suffix: v01::DerivationIndex::Index(0),
			},
			ring_location: ring_location.clone(),
			message: b"prove me".to_vec(),
		},
	)
}

fn full_person_ring_location() -> v01::RingLocation {
	v01::RingLocation {
		chain_id: [0x22; 32],
		junctions: vec![
			v01::RingLocationJunction::PalletInstance(42),
			v01::RingLocationJunction::CollectionId(b"pop:polkadot.network/people     ".to_vec()),
		],
	}
}

fn register_full_person_key(
	authority: &SigningHostRole,
	session: &AuthoritySession,
	ring: &v01::RingLocation,
) {
	futures::executor::block_on(authority.register_ring_vrf_key(
		&CallContext::default(),
		session,
		ProductRequest {
			calling_product_id: ProductId::for_test("peopl.dot"),
			payload: HostAccountRegisterRingVrfKeyRequest {
				index: v01::DerivationIndex::Index(0),
				ring: ring.clone(),
			},
		},
	))
	.expect("full person key registration succeeds");
}

#[test]
fn ring_alias_and_proof_share_the_explicit_registered_key() {
	let resolver = full_person_ring_resolver();
	let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
	let authority = SigningHostRole::new_with_ring_resolver(platform, resolver);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let cx = CallContext::default();
	let context = v01::ProductProofContext {
		product_id: ProductId::for_test("myapp.dot"),
		suffix: v01::DerivationIndex::Index(0),
	};
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	let alias = futures::executor::block_on(authority.account_alias(
		&cx,
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("peopl.dot"),
			payload: HostAccountGetAliasRequest {
				key_handle: full_person_key_handle(),
				context: context.clone(),
				ring_location: ring_location.clone(),
			},
		},
	))
	.expect("alias succeeds");
	let proof = futures::executor::block_on(authority.create_proof(
		&cx,
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("peopl.dot"),
			payload: HostAccountCreateProofRequest {
				key_handle: full_person_key_handle(),
				context,
				ring_location,
				message: b"prove me".to_vec(),
			},
		},
	))
	.expect("proof succeeds");

	assert!(!proof.proof.is_empty());
	assert_eq!(proof.contextual_alias, alias);
	assert_eq!(proof.ring_index, 7);
	assert_eq!(proof.ring_revision, 11);
}

#[test]
fn alias_checks_the_exact_registry_ring_before_resolving_it() {
	let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
	let authority = SigningHostRole::new_with_ring_resolver(platform, full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let registered_ring = full_person_ring_location();
	register_full_person_key(&authority, &session, &registered_ring);

	let error = futures::executor::block_on(authority.account_alias(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("peopl.dot"),
			payload: HostAccountGetAliasRequest {
				key_handle: full_person_key_handle(),
				context: v01::ProductProofContext {
					product_id: ProductId::for_test("myapp.dot"),
					suffix: v01::DerivationIndex::Index(0),
				},
				ring_location: v01::RingLocation {
					chain_id: registered_ring.chain_id,
					junctions: vec![],
				},
			},
		},
	))
	.unwrap_err();

	assert_eq!(error, RingVrfError::KeyNotInRing);
}

#[test]
fn direct_signing_rejects_registry_public_key_mismatched_with_wallet() {
	let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform::default());
	let authority = SigningHostRole::new_with_ring_resolver(platform, full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let handle = v01::ProductAccountId {
		dot_ns_identifier: ProductId::for_test("myapp.dot"),
		derivation_index: v01::DerivationIndex::Index(8),
	};
	futures::executor::block_on(authority.ring_vrf_registry.register(
		session.public_key,
		handle.clone(),
		full_person_ring_location(),
		[0xFF; 32],
	))
	.expect("synthetic registry entry persists");

	let error = futures::executor::block_on(authority.ring_vrf_sign(
		&CallContext::default(),
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			payload: HostAccountRingVrfSignRequest {
				key_handle: handle,
				message: b"reject mismatched registry state".to_vec(),
			},
		},
	))
	.unwrap_err();

	assert!(matches!(
		error,
		RingVrfError::Unknown { reason } if reason.contains("does not match the active wallet")
	));
}

#[test]
fn foreign_alias_prompts_but_foreign_proof_is_refused_without_a_prompt() {
	let platform = Arc::new(StubPlatform::default());
	let authority =
		SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let cx = CallContext::default();
	let context = v01::ProductProofContext {
		product_id: ProductId::for_test("other.dot"),
		suffix: v01::DerivationIndex::Index(0),
	};
	let ring_location = full_person_ring_location();
	register_full_person_key(&authority, &session, &ring_location);

	let alias = futures::executor::block_on(authority.account_alias(
		&cx,
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			payload: HostAccountGetAliasRequest {
				key_handle: full_person_key_handle(),
				context: context.clone(),
				ring_location: ring_location.clone(),
			},
		},
	));
	assert_eq!(alias, Err(RingVrfError::Rejected));

	let proof = futures::executor::block_on(authority.create_proof(
		&cx,
		&session,
		ProductRequest {
			calling_product_id: ProductId::for_test("myapp.dot"),
			payload: HostAccountCreateProofRequest {
				key_handle: full_person_key_handle(),
				context,
				ring_location,
				message: b"prove me".to_vec(),
			},
		},
	));
	assert_eq!(proof, Err(RingVrfError::NotAllowlisted));
	assert_eq!(
		platform
			.account_access_reviews
			.lock()
			.expect("account access review list mutex poisoned")
			.len(),
		1
	);
}

#[test]
fn cross_product_alias_reuses_persisted_account_access_grant() {
	let platform =
		Arc::new(StubPlatform { account_access_confirmed: true, ..StubPlatform::default() });
	let authority =
		SigningHostRole::new_with_ring_resolver(platform.clone(), full_person_ring_resolver());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let cx = CallContext::default();
	let request = ProductRequest {
		calling_product_id: ProductId::for_test("myapp.dot"),
		payload: HostAccountGetAliasRequest {
			key_handle: full_person_key_handle(),
			context: v01::ProductProofContext {
				product_id: ProductId::for_test("other.dot"),
				suffix: v01::DerivationIndex::Index(0),
			},
			ring_location: full_person_ring_location(),
		},
	};
	register_full_person_key(&authority, &session, &request.payload.ring_location);

	futures::executor::block_on(authority.account_alias(&cx, &session, request.clone()))
		.expect("first alias succeeds");
	futures::executor::block_on(authority.account_alias(&cx, &session, request))
		.expect("second alias succeeds from cached grant");

	assert_eq!(
		platform
			.account_access_reviews
			.lock()
			.expect("account access review list mutex poisoned")
			.len(),
		1
	);
}

#[test]
fn local_activation_exposes_the_peopl_dot_identity_account() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");

	let session = authority.current_session().expect("active session");
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX)
		.expect("peopl.dot identity derivation")
		.public
		.to_bytes();
	assert_eq!(session.identity_account_id, Some(identity));
}

#[test]
fn activate_then_sign_raw_verifies_against_derived_product_key() {
	let (services, activation) = signing_runtime();
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, activation);
	let cx = CallContext::default();

	let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: b"hello world".to_vec() },
	});
	let HostSignRawResponse::V1(response) =
		futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
	assert!(response.signed_transaction.is_none());

	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let signature =
		schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
	assert!(
		keypair
			.public
			.verify_simple(b"substrate", b"<Bytes>hello world</Bytes>", &signature)
			.is_ok(),
		"signature verifies over the <Bytes>-wrapped message",
	);
}

#[test]
fn sign_vrf_replays_transcript_and_returns_verifiable_proof() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let request = v01::HostAccountSignVrfRequest {
		account: product_account(0),
		transcript_label: b"pop:airdrop".to_vec(),
		items: vec![
			v01::VrfTranscriptItem { label: b"domain".to_vec(), value: b"lottery".to_vec() },
			v01::VrfTranscriptItem { label: b"round".to_vec(), value: 7u32.to_le_bytes().to_vec() },
		],
	};

	let signature = futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&session,
		&ProductId::for_test("myapp.dot"),
		request,
	))
	.expect("VRF signing succeeds");

	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let mut transcript = merlin::Transcript::new(b"pop:airdrop");
	transcript.append_message(b"domain", b"lottery");
	transcript.append_message(b"round", &7u32.to_le_bytes());
	let pre_output = schnorrkel::vrf::VRFPreOut::from_bytes(&signature.pre_output).unwrap();
	let proof = schnorrkel::vrf::VRFProof::from_bytes(&signature.proof).unwrap();
	keypair
		.public
		.vrf_verify(transcript, &pre_output, &proof)
		.expect("VRF proof verifies");
}

#[test]
fn approved_auto_signing_product_skips_vrf_confirmation_but_other_product_does_not() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		sign_vrf_confirmed: false,
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, authority.clone());
	let allocation = futures::executor::block_on(ResourceAllocation::request(
		&runtime,
		&CallContext::default(),
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	))
	.expect("approved AutoSigning allocation succeeds");
	let HostRequestResourceAllocationResponse::V1(allocation) = allocation;
	assert_eq!(allocation.outcomes, vec![v01::AllocationOutcome::Allocated],);
	assert_eq!(
		platform
			.resource_allocation_reviews
			.lock()
			.expect("resource allocation review list mutex poisoned")
			.len(),
		1,
	);

	let session = authority.current_session().expect("active session");
	futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&session,
		&ProductId::for_test("myapp.dot"),
		vrf_request("myapp.dot"),
	))
	.expect("granted product signs without another confirmation");
	assert!(
		platform
			.sign_vrf_reviews
			.lock()
			.expect("VRF signing review list mutex poisoned")
			.is_empty(),
		"the allocation grant bypasses only the subsequent VRF prompt",
	);

	let error = futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&session,
		&ProductId::for_test("other.dot"),
		vrf_request("myapp.dot"),
	))
	.expect_err("different calling product remains confirmation-bound");
	assert_eq!(error, AuthorityError::Rejected);
	let reviews = platform
		.sign_vrf_reviews
		.lock()
		.expect("VRF signing review list mutex poisoned");
	assert_eq!(reviews.len(), 1);
	assert_eq!(reviews[0].calling_product_id, ProductId::for_test("other.dot"));
}

/// A product reset revokes that product's grant and fences grant work begun
/// before it, while the wallet session and other products' grants survive.
#[test]
fn product_clear_revokes_only_current_activation_grant_and_fences_stale_work() {
	let platform =
		Arc::new(StubPlatform { resource_allocation_confirmed: true, ..StubPlatform::default() });
	let (services, authority) = signing_runtime_with_platform(platform);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec())).unwrap();
	let authority_session = authority.current_session().unwrap();
	let revision = authority.grants.lifecycle().revision();
	for product in ["myapp.dot", "other.dot"] {
		auto_signing::grant_auto_signing(&product_runtime_for(
			services.clone(),
			authority.clone(),
			product,
		));
	}
	let myapp = ProductId::for_test("myapp.dot");
	let wallet_session = authority.session_state().current().unwrap();
	let key =
		futures::executor::block_on(authority.grants.auto_signing_key(&wallet_session, &myapp))
			.unwrap()
			.unwrap();
	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let subtree =
		crate::host_logic::product_account::derive_product_subtree_keypair(&root, &myapp).unwrap();
	authority.clear_product_state(&myapp);
	let current_session = authority.wallet.current_session().unwrap();
	let own = keeps_auto_signing(&authority, "myapp.dot");
	let other = keeps_auto_signing(&authority, "other.dot");
	let stale = futures::executor::block_on(authority.grants.remember_auto_signing_key(
		&wallet_session,
		revision,
		&myapp,
		subtree.public.to_bytes(),
		key,
	));
	assert_eq!(
		(current_session.validation_id, own, other, stale),
		(authority_session.validation_id, false, true, Err(AuthorityError::Disconnected)),
	);
}

#[test]
fn auto_signing_grant_does_not_cross_root_identity_replacement() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		sign_vrf_confirmed: false,
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation succeeds");
	let runtime = product_runtime(services, authority.clone());
	futures::executor::block_on(ResourceAllocation::request(
		&runtime,
		&CallContext::default(),
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	))
	.expect("AutoSigning allocation succeeds");

	futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
		.expect("replacement activation succeeds");
	let replacement = authority.current_session().expect("replacement session");
	let error = futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&replacement,
		&ProductId::for_test("myapp.dot"),
		vrf_request("myapp.dot"),
	))
	.expect_err("replacement root must receive its own confirmation");
	assert_eq!(error, AuthorityError::Rejected);
	assert_eq!(
		platform
			.sign_vrf_reviews
			.lock()
			.expect("VRF signing review list mutex poisoned")
			.len(),
		1,
	);
}

#[test]
fn auto_signing_grant_does_not_cross_disconnect_and_same_wallet_reactivation() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		sign_vrf_confirmed: false,
		..StubPlatform::default()
	});
	let (services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation succeeds");
	let runtime = product_runtime(services, authority.clone());
	futures::executor::block_on(ResourceAllocation::request(
		&runtime,
		&CallContext::default(),
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	))
	.expect("AutoSigning allocation succeeds");

	futures::executor::block_on(authority.disconnect());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("same wallet reactivation succeeds");
	let reactivated = authority.current_session().expect("reactivated session");
	let error = futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&reactivated,
		&ProductId::for_test("myapp.dot"),
		vrf_request("myapp.dot"),
	))
	.expect_err("reactivated wallet must receive its own confirmation");
	assert_eq!(error, AuthorityError::Rejected);
	assert_eq!(
		platform
			.sign_vrf_reviews
			.lock()
			.expect("VRF signing review list mutex poisoned")
			.len(),
		1,
	);
}

#[test]
fn stale_auto_signing_completion_cannot_grant_same_wallet_reactivation() {
	let platform = Arc::new(StubPlatform { sign_vrf_confirmed: false, ..StubPlatform::default() });
	let (_services, authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation succeeds");
	let stale = authority.current_session().expect("first session snapshot");

	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("same wallet replacement activation succeeds");
	let current = authority.current_session().expect("replacement session");
	assert_ne!(stale.validation_id, current.validation_id);

	let error = futures::executor::block_on(authority.allocate_resources(
		&CallContext::default(),
		&stale,
		authority.grant_revision(),
		&ProductId::for_test("myapp.dot"),
		v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		},
	))
	.expect_err("completion captured from the old activation is stale");
	assert_eq!(error, AuthorityError::Disconnected);

	let error = futures::executor::block_on(authority.sign_vrf(
		&CallContext::default(),
		&current,
		&ProductId::for_test("myapp.dot"),
		vrf_request("myapp.dot"),
	))
	.expect_err("stale allocation must not grant the replacement activation");
	assert_eq!(error, AuthorityError::Rejected);
	assert_eq!(
		platform
			.sign_vrf_reviews
			.lock()
			.expect("VRF signing review list mutex poisoned")
			.len(),
		1,
	);
}

#[test]
fn auto_signing_grant_does_not_cross_runtime_instance() {
	let platform = Arc::new(StubPlatform {
		resource_allocation_confirmed: true,
		sign_vrf_confirmed: false,
		..StubPlatform::default()
	});
	let (services, granting_authority) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(granting_authority.activate_local_session(ENTROPY.to_vec()))
		.expect("granting runtime activates");
	let granting_runtime = product_runtime(services, granting_authority);
	futures::executor::block_on(ResourceAllocation::request(
		&granting_runtime,
		&CallContext::default(),
		HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		}),
	))
	.expect("AutoSigning allocation succeeds");

	let (_replacement_services, replacement) = signing_runtime_with_platform(platform.clone());
	futures::executor::block_on(replacement.activate_local_session(ENTROPY.to_vec()))
		.expect("replacement runtime activates with the same root");
	let session = replacement.current_session().expect("replacement session");
	let error = futures::executor::block_on(replacement.sign_vrf(
		&CallContext::default(),
		&session,
		&ProductId::for_test("myapp.dot"),
		vrf_request("myapp.dot"),
	))
	.expect_err("a separate runtime must receive its own confirmation");
	assert_eq!(error, AuthorityError::Rejected);
	assert_eq!(
		platform
			.sign_vrf_reviews
			.lock()
			.expect("VRF signing review list mutex poisoned")
			.len(),
		1,
	);
}

#[test]
fn sign_payload_product_and_legacy_use_the_substrate_preimage() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let cx = CallContext::default();
	let mut payload = crate::test_support::sign_payload_data();
	payload.signed_extensions = vec![
		"CheckSpecVersion".to_string(),
		"CheckTxVersion".to_string(),
		"CheckGenesis".to_string(),
		"CheckMortality".to_string(),
		"CheckNonce".to_string(),
		"ChargeTransactionPayment".to_string(),
	];
	payload.with_signed_transaction = parity_scale_codec::OptionBool(Some(true));
	let preimage = extrinsic_payload_preimage(&payload).expect("preimage builds");

	let product_response = futures::executor::block_on(authority.sign_payload(
		&cx,
		&session,
		None,
		SignPayloadAuthorityRequest::Product(v01::HostSignPayloadRequest {
			account: product_account(0),
			payload: payload.clone(),
		}),
	))
	.expect("product payload signing succeeds");

	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	assert_eq!(product_response.signature.len(), 65);
	assert_eq!(product_response.signature[0], 1);
	let signature = schnorrkel::Signature::from_bytes(&product_response.signature[1..]).unwrap();
	assert!(
		keypair
			.public
			.verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
			.is_ok()
	);
	let signed_transaction = product_response
		.signed_transaction
		.as_ref()
		.expect("requested signed transaction");
	let (account, embedded_signature, tail) = split_v4(signed_transaction);
	assert_eq!(account, keypair.public.to_bytes());
	assert_eq!(embedded_signature.as_slice(), &product_response.signature[1..]);
	let extensions = extrinsic_payload_extensions(&payload).unwrap();
	let expected_tail = extensions
		.iter()
		.flat_map(|extension| extension.extra.iter().copied())
		.chain(payload.method.iter().copied())
		.collect::<Vec<_>>();
	assert_eq!(tail, expected_tail);

	let legacy_response = futures::executor::block_on(authority.sign_payload(
		&cx,
		&session,
		None,
		SignPayloadAuthorityRequest::LegacyAccount {
			product_account: product_account(0),
			request: v01::HostSignPayloadWithLegacyAccountRequest {
				signer: format!("0x{}", hex::encode(keypair.public.to_bytes())),
				payload,
			},
		},
	))
	.expect("legacy payload signing succeeds");
	assert_eq!(legacy_response.signature[0], 1);
	let signature = schnorrkel::Signature::from_bytes(&legacy_response.signature[1..]).unwrap();
	assert!(
		keypair
			.public
			.verify_simple(SR25519_SIGNING_CONTEXT, &preimage, &signature)
			.is_ok()
	);
	assert!(legacy_response.signed_transaction.is_some());
}

#[test]
fn sign_raw_legacy_accepts_only_the_peopl_dot_identity_key() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = authority.current_session().expect("active session");
	let cx = CallContext::default();
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX).unwrap();
	let request = |account| SignRawAuthorityRequest::LegacyAccount {
		account,
		request: v01::HostSignRawWithLegacyAccountRequest {
			signer: String::new(),
			payload: v01::RawPayload::Bytes { bytes: b"hello".to_vec() },
		},
	};

	let response = futures::executor::block_on(authority.sign_raw(
		&cx,
		&session,
		None,
		request(identity.public.to_bytes()),
		true,
	))
	.expect("identity raw signing succeeds");
	let signature = schnorrkel::Signature::from_bytes(&response.signature).unwrap();
	assert!(
		identity
			.public
			.verify_simple(SR25519_SIGNING_CONTEXT, b"<Bytes>hello</Bytes>", &signature)
			.is_ok()
	);

	let error = futures::executor::block_on(authority.sign_raw(
		&cx,
		&session,
		None,
		request([0xff; 32]),
		true,
	))
	.expect_err("unknown legacy account is rejected");
	assert!(matches!(error, AuthorityError::Unavailable { .. }));
}

#[test]
fn sign_raw_requires_active_session() {
	let (services, authority) = signing_runtime();
	let runtime = product_runtime(services, authority);
	let cx = CallContext::default();
	let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: vec![1, 2, 3] },
	});
	let err = futures::executor::block_on(runtime.sign_raw(&cx, request)).expect_err("no session");
	assert!(matches!(err, CallError::Domain(HostSignRawError::V1(_))));
}

fn product_account(index: u32) -> v01::ProductAccountId {
	v01::ProductAccountId {
		dot_ns_identifier: ProductId::for_test("myapp.dot"),
		derivation_index: v01::DerivationIndex::Index(index),
	}
}

fn tx_payload(tx_ext_version: u8) -> v01::ProductAccountTxPayload {
	v01::ProductAccountTxPayload {
		signer: product_account(0),
		genesis_hash: [0xaa; 32],
		call_data: vec![0x00, 0x00],
		extensions: vec![v01::TxPayloadExtension {
			id: "CheckNonce".to_string(),
			extra: vec![1],
			additional_signed: vec![2, 3],
		}],
		tx_ext_version,
		contacts: Vec::new(),
	}
}

#[test]
fn create_transaction_reaches_chain_metadata_resolution() {
	let platform: Arc<dyn crate::platform::Platform> = Arc::new(StubPlatform {
		chain_connect_error: Some("fixture has no live chain"),
		..StubPlatform::default()
	});
	let (_services, activation) = signing_runtime_with_platform(platform);
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = activation.current_session().expect("active session");
	let cx = CallContext::default();

	let err = futures::executor::block_on(activation.create_transaction(
		&cx,
		&session,
		None,
		CreateTransactionAuthorityRequest::Product(tx_payload(0)),
	))
	.expect_err("fixture cannot resolve metadata");
	assert!(
		matches!(err, AuthorityError::Unavailable { reason } if reason.contains("cannot load chain metadata")),
		"choosing the extrinsic format reads the runtime metadata"
	);
}

#[test]
fn create_transaction_legacy_signer_mismatch_errors() {
	let (_services, activation) = signing_runtime();
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let session = activation.current_session().expect("active session");
	let cx = CallContext::default();

	let payload = tx_payload(0);
	let request = CreateTransactionAuthorityRequest::LegacyAccount {
		product_account: product_account(0),
		request: v01::LegacyAccountTxPayload {
			signer: [0xff; 32], // does not match the derived slot-zero key
			genesis_hash: payload.genesis_hash,
			call_data: payload.call_data.clone(),
			extensions: payload.extensions.clone(),
			tx_ext_version: 0,
		},
	};
	let err =
		futures::executor::block_on(activation.create_transaction(&cx, &session, None, request))
			.expect_err("mismatched legacy signer");
	assert!(matches!(err, AuthorityError::Unknown { reason } if reason.contains("does not match")));
}

#[test]
fn create_transaction_requires_active_session() {
	let (_services, activation) = signing_runtime();
	// A session snapshot cannot exist without activation, so construct the
	// request against a role that has never been activated.
	let (_s2, other) = signing_runtime();
	futures::executor::block_on(other.activate_local_session(ENTROPY.to_vec())).unwrap();
	let stale_session = other.current_session().expect("session");
	futures::executor::block_on(other.disconnect());
	let cx = CallContext::default();

	let err = futures::executor::block_on(activation.create_transaction(
		&cx,
		&stale_session,
		None,
		CreateTransactionAuthorityRequest::Product(tx_payload(0)),
	))
	.expect_err("no active session");
	assert_eq!(err, AuthorityError::Disconnected);
}

#[test]
fn derive_entropy_matches_ios_vector_over_local_session() {
	let (services, activation) = signing_runtime();
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime_for(services, activation, "test.product.dot");
	let cx = CallContext::default();
	let request =
		HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context: b"my-key".to_vec() });
	let response = futures::executor::block_on(runtime.derive(&cx, request)).expect("derive ok");
	let truapi::versioned::entropy::HostDeriveEntropyResponse::V1(inner) = response;
	assert_eq!(
		hex::encode(inner.entropy),
		"479d5b9ecce19615397c9f160ee95e2f00c579837a5afb111132dd0da5fd472a",
	);
}

#[test]
fn get_account_gates_on_local_session() {
	let (services, authority) = signing_runtime();
	let runtime = product_runtime(services, authority);
	let cx = CallContext::default();
	let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
		product_account_id: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
	});
	let err =
		futures::executor::block_on(runtime.get_account(&cx, request)).expect_err("no session yet");
	assert!(matches!(
		err,
		CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::NotConnected))
	));
}

#[test]
fn sign_raw_leaves_already_wrapped_payload_untouched() {
	let (services, activation) = signing_runtime();
	futures::executor::block_on(activation.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	let runtime = product_runtime(services, activation);
	let cx = CallContext::default();
	let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: b"<Bytes>hi</Bytes>".to_vec() },
	});
	let HostSignRawResponse::V1(response) =
		futures::executor::block_on(runtime.sign_raw(&cx, request)).expect("sign_raw ok");
	let root = derive_root_keypair_from_entropy(&ENTROPY).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let signature =
		schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");
	assert!(
		keypair
			.public
			.verify_simple(b"substrate", b"<Bytes>hi</Bytes>", &signature)
			.is_ok(),
		"signature verifies over the unchanged wrapped message",
	);
	assert!(
		keypair
			.public
			.verify_simple(b"substrate", b"<Bytes><Bytes>hi</Bytes></Bytes>", &signature)
			.is_err(),
		"payload was not double-wrapped",
	);
}

#[test]
fn reactivation_invalidates_prior_session_snapshot() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("first activation");
	let stale = authority.current_session().expect("snapshot");

	// Re-activate with different entropy: a fresh public key, hence a
	// different validation id.
	futures::executor::block_on(authority.activate_local_session([0xCD; 16].to_vec()))
		.expect("second activation");
	assert_ne!(authority.current_session().expect("session").public_key, stale.public_key,);

	let cx = CallContext::default();
	let request = v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: vec![1, 2, 3] },
	};
	let err = futures::executor::block_on(authority.sign_raw(
		&cx,
		&stale,
		None,
		SignRawAuthorityRequest::Product(request),
		true,
	))
	.expect_err("stale snapshot rejected");
	assert_eq!(err, AuthorityError::Disconnected);
}

#[test]
fn disconnect_clears_local_session() {
	let (_services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation");
	let session = authority.current_session().expect("connected");

	futures::executor::block_on(authority.disconnect());
	assert!(authority.current_session().is_none());

	let cx = CallContext::default();
	let request = v01::HostSignRawRequest {
		account: v01::ProductAccountId {
			dot_ns_identifier: ProductId::for_test("myapp.dot"),
			derivation_index: v01::DerivationIndex::Index(0),
		},
		payload: v01::RawPayload::Bytes { bytes: vec![1] },
	};
	let err = futures::executor::block_on(authority.sign_raw(
		&cx,
		&session,
		None,
		SignRawAuthorityRequest::Product(request),
		true,
	))
	.expect_err("no session after disconnect");
	assert_eq!(err, AuthorityError::Disconnected);
}

/// A PGAS request has to actually reach Asset Hub. Asserting the outcome alone
/// cannot show that: the stub this replaced answered `NotAvailable` with no
/// chain access at all, which looks identical from outside. Pin instead that
/// the request is left waiting on the connection.
#[test]
fn a_pgas_request_waits_on_the_asset_hub_connection() {
	use std::{
		future::Future,
		task::{Context, Poll},
	};

	let platform =
		Arc::new(StubPlatform { chain_connect_pending: true, ..StubPlatform::default() });
	let (_services, authority) = signing_runtime_on_asset_hub(platform, Some([0xcc; 32]));
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation");
	let session = authority.current_session().expect("connected");
	let cx = CallContext::default();
	let product_id = ProductId::for_test("myapp.dot");

	let mut allocation = Box::pin(authority.allocate_resources(
		&cx,
		&session,
		authority.grant_revision(),
		&product_id,
		v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::SmartContractAllowance(
				v01::DerivationIndex::Index(0),
			)],
		},
	));
	let waker = futures::task::noop_waker();
	let mut task_cx = Context::from_waker(&waker);

	match allocation.as_mut().poll(&mut task_cx) {
		Poll::Pending => {},
		Poll::Ready(outcome) => {
			panic!("a PGAS claim should be waiting on Asset Hub, got {outcome:?}")
		},
	}
}

/// Each allocation spends on chain, so a withdrawn call must not start the
/// next one.
#[test]
fn a_withdrawn_allocation_starts_no_further_resource() {
	let (_services, authority) = signing_runtime_with_platform(Arc::new(StubPlatform::default()));
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation");
	let session = authority.current_session().expect("connected");
	let cancel = truapi::CancellationToken::default();
	cancel.cancel();
	let cx = CallContext::with_parts("allocation-withdrawn".to_string(), cancel);

	let result = futures::executor::block_on(authority.allocate_resources(
		&cx,
		&session,
		authority.grant_revision(),
		&ProductId::for_test("myapp.dot"),
		v01::HostRequestResourceAllocationRequest {
			resources: vec![v01::AllocatableResource::AutoSigning],
		},
	));

	assert_eq!(
		result,
		Err(AuthorityError::Cancelled(crate::runtime::authority::AuthorityCancelError::new(
			"allocation-withdrawn",
			truapi::CancellationReason::Cancelled,
		)))
	);
}

#[test]
fn direct_allocation_handles_empty_and_optional_resource_batches() {
	// A PGAS request now reaches Asset Hub, so the stub has to fail the
	// connect for this to be deterministic. What the batch pins is that one
	// resource failing does not poison the others.
	let platform = Arc::new(StubPlatform {
		chain_connect_error: Some("asset hub unavailable"),
		..StubPlatform::default()
	});
	let (_services, authority) = signing_runtime_with_platform(platform);
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation");
	let session = authority.current_session().expect("connected");
	let cx = CallContext::default();

	let empty = futures::executor::block_on(authority.allocate_resources(
		&cx,
		&session,
		authority.grant_revision(),
		&ProductId::for_test("myapp.dot"),
		v01::HostRequestResourceAllocationRequest { resources: vec![] },
	))
	.expect("empty allocation succeeds");
	assert!(empty.outcomes.is_empty());

	let optional = futures::executor::block_on(authority.allocate_resources(
		&cx,
		&session,
		authority.grant_revision(),
		&ProductId::for_test("myapp.dot"),
		v01::HostRequestResourceAllocationRequest {
			resources: vec![
				v01::AllocatableResource::SmartContractAllowance(v01::DerivationIndex::Index(0)),
				v01::AllocatableResource::AutoSigning,
			],
		},
	))
	.expect("optional allocation succeeds");
	assert_eq!(
		optional.outcomes,
		vec![
			// The chain is unreachable, so the claim degrades rather than
			// failing the request.
			v01::AllocationOutcome::NotAvailable,
			// And the resource that needs no chain still allocates.
			v01::AllocationOutcome::Allocated,
		]
	);
}
