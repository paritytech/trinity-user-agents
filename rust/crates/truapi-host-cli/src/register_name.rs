//! Full-person username registration through `DotnsGateway.register_name`.
//!
//! Builds and submits the v5 general transaction on Asset Hub. The signer's
//! RFC-0022 `uid.<suffix>` account is the call's `who`. The full-person
//! bandersnatch key proves People-ring membership bound to the dotNS gateway
//! context. A fresh
//! sr25519 signature over the inherited-implication digest travels in the
//! `AsDotnsGateway` extension.
//!
//! Requires a recognized full person. The account's full-person key must be
//! `Included` in a People ring already propagated to Asset Hub.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use parity_scale_codec::{Compact, Decode, Encode};
use subxt::{tx::Signer, utils::H256};
use tracing::debug;
use truapi::{
	Sr25519Signer,
	chain::MortalExtrinsic,
	host_logic::{
		dotns_gateway::{
			AsDotnsGatewayExtension, ClaimBlocker, DOTNS_GATEWAY_CONTEXT, Link, MAX_BASE_LABEL_LEN,
			MIN_PERSON_LABEL_LEN, RegisterNameCallArgs, ReviveCallArgs, ReviveDryRun, WeightLimit,
			account_to_h160, build_register_proof_message, expire_reservation_call,
			is_dotted_lite_username, is_registrable_full_label,
		},
		product_account::{derive_full_person_ring_vrf_entropy, derive_identity_keypair},
	},
	statement_allowance::{
		self as alloc, CollectionCandidate, ExtrinsicTarget, FixedPersonhoodSigner,
		PersonhoodSigner, collection::PersonhoodCollection, extension::Metadata, ring,
		rpc::RpcClient,
	},
};

use crate::{
	accounts::IdentityDerivationError, dotns_read::AssetHubReader, network::NetworkConfig,
};

/// Inputs for one full-person registration run.
pub struct RegisterNameConfig {
	/// Network preset providing the People and Asset Hub endpoints.
	pub network: NetworkConfig,
	/// BIP-39 entropy of the signing host's root account.
	pub entropy: Vec<u8>,
	/// Base label to register: lowercase ASCII letters only (`is_person_label`
	/// in the gateway pallet), at most 32 bytes.
	pub label: String,
	/// Dotted lite username to link (`name.NN`). Without this and without
	/// `chat_key`, the account's own lite username is looked up and linked.
	pub link_lite: Option<String>,
	/// 65-byte ECDH chat key for a standalone (unlinked) registration.
	pub chat_key: Option<[u8; 65]>,
}

/// Registers (or claims) `label` as a full-person username. Waits until the
/// gateway records the account's alias on Asset Hub.
pub async fn register_name(config: &RegisterNameConfig) -> Result<()> {
	// The pallet and the contract reject malformed labels only at submission
	// (as an invalid transaction or a revert), so check the cheap rules here.
	let label = &config.label;
	if !is_registrable_full_label(label) {
		bail!(
			"label {label:?} is not a registrable full-person base label: lowercase ASCII \
             letters only, {MIN_PERSON_LABEL_LEN} to {MAX_BASE_LABEL_LEN} bytes"
		);
	}
	if let Some(lite) = &config.link_lite &&
		!is_dotted_lite_username(lite)
	{
		bail!("--link-lite {lite:?} is not a dotted lite username (`name.NN`)");
	}
	let who = derive_identity_keypair(&config.entropy, config.network.network_suffix)
		.map_err(IdentityDerivationError::from)?;
	let who_public = who.public.to_bytes();

	let mut reader = AssetHubReader::for_network(&config.network).await?;
	// Registered names (any flow) fail only at submission otherwise.
	if !reader.label_available(label).await? {
		bail!("label {label:?} is already registered on dotNS and cannot be registered again");
	}
	// A claim the reservation queue refuses reverts in the gateway call, and the
	// alias wait below would only time out. Too many expired or emptied
	// reservations in front of the claim are cleared before it is built.
	let clear_queue_first = match reader.claim_blocker(label, &account_to_h160(&who_public)).await?
	{
		None => false,
		Some(ClaimBlocker::HeldBy(holder)) => return Err(held_by_other(label, &holder)),
		Some(ClaimBlocker::ClearFirst { dead }) => {
			println!("REGISTER_QUEUE_BLOCKED label={label} dead={dead}");
			true
		},
	};
	// The gateway rejects a second registration for the account and a lite
	// link the account does not own; both are storage reads, so ask first.
	if reader.account_alias(&who_public).await?.is_some() {
		bail!("this account has already registered a full-person username on the gateway");
	}
	let link = resolve_link(config, &mut reader, &who_public).await?;
	if let Link::LiteUsername(lite) = &link {
		let lite = core::str::from_utf8(lite).unwrap_or_default();
		match reader.lite_label_owner(lite).await? {
			Some(owner) if owner == who_public => {},
			Some(_) => {
				bail!(
					"lite username {lite:?} was reserved for another account; pass --chat-key for a \
                 standalone registration"
				)
			},
			None => {
				bail!(
					"lite username {lite:?} was not reserved through the dotNS gateway on this \
                 network; pass --chat-key for a standalone registration"
				)
			},
		}
	}

	// Reading the full-person member key's ring and its members from the People
	// chain, pinned to one finalized block.
	let people_rpc = RpcClient::connect(config.network.people_ws)
		.await
		.context("connect People RPC")?;
	let people_metadata = alloc::fetch_metadata(&people_rpc).await?;
	let at = people_rpc.finalized_head().await?;
	let full_entropy =
		derive_full_person_ring_vrf_entropy(&config.entropy, config.network.network_suffix);
	let candidates =
		[CollectionCandidate { collection: PersonhoodCollection::People, entropy: full_entropy }];
	let signer = FixedPersonhoodSigner::new(&candidates).await?;
	let member = signer.member(PersonhoodCollection::People)?;
	let ring_index = ring::read_member_ring_index_at(
		&people_rpc,
		&people_metadata,
		PersonhoodCollection::People,
		&member,
		&at,
	)
	.await
	.context("resolve full-person ring membership (is the account a recognized person?)")?;
	let members =
		ring::read_ring_members_at(&people_rpc, PersonhoodCollection::People, ring_index, &at)
			.await?;
	// `Members.Members` reports a ring index as soon as the key is onboarded;
	// the members read above is sliced to the keys already built into the
	// ring's root, so a fresh member can be missing from it for a few blocks.
	if !members.contains(&member) {
		bail!(
			"the account's full-person key is onboarded into People ring {ring_index} but \
             not yet built into its root; retry once the next root revision is built"
		);
	}
	// The revision of the root the proof is built against, at the same pinned
	// block as the members read, so the two cannot disagree.
	let revision = ring::read_ring_revision(
		&people_rpc,
		&people_metadata,
		PersonhoodCollection::People,
		ring_index,
		&at,
	)
	.await?;
	debug!(ring_index, revision, members = members.len(), "full-person ring resolved");

	// Reading Asset Hub metadata and the exponent the subscriber verifies
	// proofs against.
	let ah_rpc = RpcClient::connect(config.network.asset_hub_ws)
		.await
		.context("connect Asset Hub RPC")?;
	let ah_metadata = alloc::fetch_metadata(&ah_rpc).await?;
	let exponent =
		ring::read_subscriber_ring_exponent(&ah_rpc, &ah_metadata, PersonhoodCollection::People)
			.await?;

	// Building the ring-VRF membership proof. It is bound to the gateway context
	// and the registration intent, not to the extrinsic, so it is built once.
	let proof_message = build_register_proof_message(&who_public, config.label.as_bytes(), &link);
	let ring_params = ring::RingParams {
		collection: PersonhoodCollection::People,
		members,
		exponent,
		ring_index,
		block_hash: at,
	};
	let ring_proof = signer.prove(&ring_params, &DOTNS_GATEWAY_CONTEXT, &proof_message)?;

	// Asset Hub only verifies proofs against root revisions it has imported
	// from People; wait for this one before building, so the extrinsic is born
	// after the wait.
	alloc::pgas::await_ring_revision(
		&ah_rpc,
		&ah_metadata,
		PersonhoodCollection::People,
		ring_index,
		revision,
	)
	.await
	.context("wait for Asset Hub to import the People ring root revision")?;

	let extrinsics = crate::extrinsic_service(&config.network);
	let asset_hub = ExtrinsicTarget {
		extrinsics: &extrinsics,
		genesis: H256(config.network.asset_hub_genesis),
	};
	let who_signer = Sr25519Signer::from_keypair(&who);
	if clear_queue_first {
		clear_reservation_queue(QueueClear {
			reader: &mut reader,
			label,
			who: &who_signer,
			asset_hub_rpc: &ah_rpc,
			asset_hub_metadata: &ah_metadata,
			asset_hub,
			people_rpc: &people_rpc,
			people_metadata: &people_metadata,
			signer: &signer,
			ring: &ring_params,
		})
		.await?;
	}

	// The account signs the implication with sr25519; the signature travels
	// next to the ring proof in the AsDotnsGateway value.
	let authorizer = AsDotnsGatewayExtension { ring_proof, ring_index, revision, who: &who_signer };
	let call =
		RegisterNameCallArgs { who: who_public, label: config.label.as_bytes().to_vec(), link };
	let extrinsic = extrinsics
		.build_mortal_extrinsic(H256(config.network.asset_hub_genesis))
		.with_call_args("DotnsGateway", "register_name", call)
		.with_extension(authorizer)
		.with_restrict_origins(true)
		.build()
		.await?;

	let block = ah_rpc.submit_and_watch(extrinsic.extrinsic.bytes()).await?;
	println!("REGISTER_SUBMITTED label={} block={block}", config.label);

	wait_for_alias(&reader, &who_public).await?;
	let identity = reader.dotns_identity(&who_public).await?;
	println!(
		"REGISTER_CONFIRMED label={} full_username={}",
		config.label,
		identity
			.as_ref()
			.and_then(|identity| identity.full_username())
			.unwrap_or("<pending>")
	);
	Ok(())
}

/// What clearing a name's reservation queue before the claim needs.
struct QueueClear<'a> {
	reader: &'a mut AssetHubReader,
	label: &'a str,
	/// The claiming account, which signs the clearing call and pays its fee.
	who: &'a Sr25519Signer,
	asset_hub_rpc: &'a RpcClient,
	asset_hub_metadata: &'a Metadata,
	asset_hub: ExtrinsicTarget<'a>,
	people_rpc: &'a RpcClient,
	people_metadata: &'a Metadata,
	signer: &'a dyn PersonhoodSigner,
	/// The account's People ring, which a PGAS claim proves membership of.
	ring: &'a ring::RingParams,
}

/// Clears the expired and emptied reservations in front of the claim with a
/// `DotnsPopController.expireReservation(label)` signed by the claiming account.
///
/// A gateway claim clears at most `MAX_QUEUE_STEPS_PER_CALL` of them and a
/// reverted claim keeps none of that, so a longer run has to be cleared first.
/// The fee and the storage deposit are paid in PGAS, claimed into the account
/// when its balance falls short. A dry-run from an account that cannot fund the
/// deposit fails, but still reports the limits that price the claim.
async fn clear_reservation_queue(clear: QueueClear<'_>) -> Result<()> {
	let QueueClear {
		reader,
		label,
		who,
		asset_hub_rpc,
		asset_hub_metadata,
		asset_hub,
		people_rpc,
		people_metadata,
		signer,
		ring,
	} = clear;
	let who_public = who.account_id().0;
	let clear_call = ClearCall {
		who,
		controller: reader.pop_controller().await?,
		input: expire_reservation_call(label),
		asset_hub,
	};

	let mut priced = clear_call.price(reader, asset_hub_rpc).await?;
	// A PGAS claim spends one of the day's slots, and funds only repair a
	// storage deposit the account cannot pay.
	let unfunded_deposit =
		asset_hub_metadata.error_indices("Revive", "StorageDepositNotEnoughFunds")?;
	if let Err(error) = &priced.dry_run.result &&
		!priced.dry_run.fails_with(unfunded_deposit)
	{
		bail!("DotnsPopController.expireReservation({label:?}) fails in a dry-run: {error}");
	}
	if alloc::pgas::pgas_balance(asset_hub_rpc, asset_hub_metadata, &who_public).await? <
		priced.cost
	{
		let network_suffix = alloc::slot::read_network_suffix(asset_hub_rpc).await?;
		let claimed = alloc::pgas::claim_pgas(alloc::pgas::PgasClaim {
			asset_hub_rpc,
			asset_hub_metadata,
			asset_hub_target: asset_hub,
			people_rpc,
			people_metadata,
			signer,
			network_suffix: &network_suffix,
			target: &who_public,
			ring,
		})
		.await
		.context("claim PGAS for the reservation queue clearing")?;
		println!("REGISTER_PGAS_CLAIMED block={}", claimed.block_hash);
		// The funded dry-run is the one that tells whether the call goes
		// through, and the claim took blocks, so the build starts over.
		priced = clear_call.price(reader, asset_hub_rpc).await?;
		let balance =
			alloc::pgas::pgas_balance(asset_hub_rpc, asset_hub_metadata, &who_public).await?;
		if balance < priced.cost {
			bail!(
				"PGAS balance {balance} does not cover the queue clearing fee and storage \
				 deposit, {}",
				priced.cost
			);
		}
	}
	if let Err(error) = &priced.dry_run.result {
		bail!("DotnsPopController.expireReservation({label:?}) fails in a dry-run: {error}");
	}

	let block = asset_hub_rpc.submit_and_watch(priced.extrinsic.extrinsic.bytes()).await?;
	// Inclusion does not mean the call succeeded, so read the queue again.
	match reader.claim_blocker(label, &account_to_h160(&who_public)).await? {
		None => {
			println!("REGISTER_QUEUE_CLEARED label={label} block={block}");
			Ok(())
		},
		Some(ClaimBlocker::HeldBy(holder)) => Err(held_by_other(label, &holder)),
		Some(ClaimBlocker::ClearFirst { dead }) => bail!(
			"DotnsPopController.expireReservation({label:?}) was included in {block} but {dead} \
			 expired or emptied reservations are still in front of this claim"
		),
	}
}

/// The signed `expireReservation` call, before its limits are known.
struct ClearCall<'a> {
	who: &'a Sr25519Signer,
	controller: [u8; 20],
	input: Vec<u8>,
	asset_hub: ExtrinsicTarget<'a>,
}

/// A clearing call built from a dry-run, and what the account has to hold in
/// PGAS to send it.
struct PricedClear {
	dry_run: ReviveDryRun,
	extrinsic: MortalExtrinsic,
	/// The transaction fee plus the storage deposit the call needs at its peak.
	/// `ChargePGAS` charges the fee in PGAS one to one, and the storage deposit
	/// is held in PGAS while the account has enough of it.
	cost: u128,
}

impl ClearCall<'_> {
	/// Dry-runs the call from the account, then builds it with the limits that
	/// reports and prices it.
	async fn price(&self, reader: &AssetHubReader, rpc: &RpcClient) -> Result<PricedClear> {
		let dry_run = reader
			.dry_run_call(&self.who.account_id().0, &self.controller, &self.input)
			.await
			.context("dry-run DotnsPopController.expireReservation")?;
		let call = ReviveCallArgs {
			dest: self.controller,
			value: 0,
			weight_limit: WeightLimit {
				ref_time: with_margin(dry_run.ref_time),
				proof_size: with_margin(dry_run.proof_size),
			},
			storage_deposit_limit: dry_run.storage_deposit,
			data: self.input.clone(),
		};
		let extrinsic = self
			.asset_hub
			.extrinsics
			.build_mortal_extrinsic(self.asset_hub.genesis)
			.with_call_args("Revive", "call", call)
			.with_signer(self.who)
			.build()
			.await?;
		let fee = query_fee(rpc, extrinsic.extrinsic.bytes()).await?;
		let cost = fee.saturating_add(dry_run.storage_deposit);
		Ok(PricedClear { dry_run, extrinsic, cost })
	}
}

/// The refusal for a name whose first active reservation is another account's.
fn held_by_other(label: &str, holder: &[u8; 20]) -> anyhow::Error {
	anyhow::anyhow!(
		"label {label:?} is reserved for 0x{} in its reservation queue; only that account can \
		 claim it",
		hex::encode(holder)
	)
}

/// `value` plus 20%, so a weight taken from a dry-run still holds when the state
/// moves a little before the call executes.
fn with_margin(value: u64) -> u64 {
	value.saturating_add(value / 5)
}

/// The fee `TransactionPaymentApi_query_info` reports for `extrinsic`.
async fn query_fee(rpc: &RpcClient, extrinsic: &[u8]) -> Result<u128> {
	let args = [extrinsic, &(extrinsic.len() as u32).encode()].concat();
	let output = rpc
		.call(
			"state_call",
			serde_json::json!([
				"TransactionPaymentApi_query_info",
				format!("0x{}", hex::encode(args))
			]),
		)
		.await?;
	let output = output.as_str().context("query_info returned a non-string response")?;
	let bytes = hex::decode(output.strip_prefix("0x").unwrap_or(output))
		.context("query_info output is not valid hex")?;
	// RuntimeDispatchInfo: compact ref_time, compact proof_size, class, partial_fee.
	let (_, _, _, fee) = <(Compact<u64>, Compact<u64>, u8, u128)>::decode(&mut &bytes[..])
		.context("decode RuntimeDispatchInfo")?;
	Ok(fee)
}

/// Resolves the `Link` argument. An explicit chat key wins, then an explicit
/// lite label, then the account's own on-chain lite username.
async fn resolve_link(
	config: &RegisterNameConfig,
	reader: &mut AssetHubReader,
	who: &[u8; 32],
) -> Result<Link> {
	if let Some(chat_key) = config.chat_key {
		return Ok(Link::None(chat_key));
	}
	if let Some(lite) = &config.link_lite {
		return Ok(Link::LiteUsername(lite.as_bytes().to_vec()));
	}
	let identity = reader.dotns_identity(who).await?;
	match identity.and_then(|identity| identity.into_lite_username()) {
		// The resolved name goes into the signed ring-VRF message, so it has
		// to pass the same shape check as an explicit --link-lite: a store can
		// hold a two-digit-suffixed label whose dotted form exceeds the
		// gateway's BaseLabel bound.
		Some(lite) if is_dotted_lite_username(&lite) => {
			debug!(%lite, "linking the account's own lite username");
			Ok(Link::LiteUsername(lite.into_bytes()))
		},
		Some(lite) => {
			bail!(
				"the account's lite username {lite:?} is not a linkable `name.NN` label; pass \
             --chat-key <hex 65 bytes> for a standalone registration"
			)
		},
		None => {
			bail!(
				"account has no lite username to link; pass --link-lite <name.NN> or \
             --chat-key <hex 65 bytes> for a standalone registration"
			)
		},
	}
}

/// Polls `DotnsGateway.AccountAlias[who]` until the registration lands.
async fn wait_for_alias(reader: &AssetHubReader, who: &[u8; 32]) -> Result<()> {
	const MAX_ATTEMPTS: usize = 10;
	for attempt in 1..=MAX_ATTEMPTS {
		if let Some(alias) = reader.account_alias(who).await? {
			println!("REGISTER_ALIAS alias=0x{}", hex::encode(alias));
			return Ok(());
		}
		debug!("DotnsGateway.AccountAlias poll {attempt}/{MAX_ATTEMPTS}: empty");
		if attempt < MAX_ATTEMPTS {
			tokio::time::sleep(Duration::from_secs(4)).await;
		}
	}
	bail!("DotnsGateway.AccountAlias did not appear after registration")
}
