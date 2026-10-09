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
use tracing::debug;
use truapi::host_logic::dotns_gateway::{
    DOTNS_GATEWAY_CONTEXT, Link, MAX_BASE_LABEL_LEN, MIN_PERSON_LABEL_LEN,
    build_register_proof_message, encode_register_full_name_extra, encode_register_name_call,
    is_dotted_lite_username, is_registrable_full_label,
};
use truapi::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derive_full_person_ring_vrf_entropy, derive_identity_keypair,
};
use truapi::statement_allowance::collection::PersonhoodCollection;
use truapi::statement_allowance::extension::AS_DOTNS_GATEWAY;
use truapi::statement_allowance::{
    self as alloc, extension, extrinsic, proof, ring, rpc::RpcClient,
};

use crate::dotns_read::AssetHubReader;
use crate::network::NetworkConfig;

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
    if let Some(lite) = &config.link_lite
        && !is_dotted_lite_username(lite)
    {
        bail!("--link-lite {lite:?} is not a dotted lite username (`name.NN`)");
    }
    let who = derive_identity_keypair(&config.entropy, config.network.network_suffix)
        .map_err(|err| anyhow::anyhow!("uid identity derivation failed: {err}"))?;
    let who_public = who.public.to_bytes();

    let mut reader = AssetHubReader::connect(config.network.asset_hub_ws).await?;
    // Registered names (any flow) fail only at submission otherwise; a pending
    // reservation of `label` is not a mint and stays claimable.
    if !reader.label_available(label).await? {
        bail!("label {label:?} is already registered on dotNS and cannot be registered again");
    }
    // The gateway rejects a second registration for the account and a lite
    // link the account does not own; both are storage reads, so ask first.
    if reader.account_alias(&who_public).await?.is_some() {
        bail!("this account has already registered a full-person username on the gateway");
    }
    let link = resolve_link(config, &mut reader, &who_public).await?;
    if let Link::LiteUsername(lite) = &link {
        let lite = core::str::from_utf8(lite).unwrap_or_default();
        match reader.lite_label_owner(lite).await? {
            Some(owner) if owner == who_public => {}
            Some(_) => {
                bail!(
                    "lite username {lite:?} was reserved for another account; pass --chat-key for a \
                 standalone registration"
                )
            }
            None => {
                bail!(
                    "lite username {lite:?} was not reserved through the dotNS gateway on this \
                 network; pass --chat-key for a standalone registration"
                )
            }
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
    let member = proof::member_key(full_entropy).await?;
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
    debug!(
        ring_index,
        revision,
        members = members.len(),
        "full-person ring resolved"
    );

    // Reading Asset Hub metadata, which asserts the RegisterFullName shape.
    // Then the chain state and the exponent the subscriber verifies proofs
    // against.
    let ah_rpc = RpcClient::connect(config.network.asset_hub_ws)
        .await
        .context("connect Asset Hub RPC")?;
    let ah_metadata = alloc::fetch_metadata(&ah_rpc).await?;
    let chain_state = extension::ChainState {
        // A restricted-origin general transaction. RestrictOrigins carries true.
        restrict_origins: true,
        ..alloc::fetch_chain_state(&ah_rpc).await?
    };
    let info_variant = ah_metadata.dotns_register_full_name_variant()?;
    let exponent =
        ring::read_subscriber_ring_exponent(&ah_rpc, &ah_metadata, PersonhoodCollection::People)
            .await?;
    let domain = proof::domain_for_ring_exponent(exponent)?;

    let call_indices = ah_metadata.call_indices("DotnsGateway", "register_name")?;
    let call = encode_register_name_call(call_indices, &who_public, config.label.as_bytes(), &link);

    // Signing the inherited-implication digest with sr25519. The signature is
    // carried in the AsDotnsGateway extension.
    let digest = extension::build_proof_message_after_extension(
        &ah_metadata,
        &call,
        &chain_state,
        AS_DOTNS_GATEWAY,
    )?;
    let signature = who
        .secret
        .sign_simple(SR25519_SIGNING_CONTEXT, &digest, &who.public)
        .to_bytes();

    // Building the ring-VRF membership proof. It is bound to the gateway context
    // and the registration intent.
    let proof_message = build_register_proof_message(&who_public, config.label.as_bytes(), &link);
    let ring_proof = proof::ring_vrf_proof(
        domain,
        full_entropy,
        &members,
        &DOTNS_GATEWAY_CONTEXT,
        &proof_message,
    )
    .await?;

    // Asset Hub only verifies proofs against root revisions it has imported
    // from People; wait for this one before submitting.
    alloc::pgas::await_ring_revision(
        &ah_rpc,
        &ah_metadata,
        PersonhoodCollection::People,
        ring_index,
        revision,
    )
    .await
    .context("wait for Asset Hub to import the People ring root revision")?;

    let extra = encode_register_full_name_extra(
        info_variant,
        &ring_proof,
        ring_index,
        revision,
        &signature,
    );
    let extrinsic = extrinsic::build_unsigned_extrinsic_with_extra(
        &ah_metadata,
        &chain_state,
        &call,
        AS_DOTNS_GATEWAY,
        &extra,
    )?;

    let block = ah_rpc.submit_and_watch(&extrinsic).await?;
    println!("REGISTER_SUBMITTED label={} block={block}", config.label);

    wait_for_alias(&reader, &who_public).await?;
    let identity = reader.dotns_identity(&who_public).await?;
    println!(
        "REGISTER_CONFIRMED label={} full_username={}",
        config.label,
        identity.full_username.as_deref().unwrap_or("<pending>")
    );
    Ok(())
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
    match identity.lite_username {
        // The resolved name goes into the signed ring-VRF message, so it has
        // to pass the same shape check as an explicit --link-lite: a store can
        // hold a two-digit-suffixed label whose dotted form exceeds the
        // gateway's BaseLabel bound.
        Some(lite) if is_dotted_lite_username(&lite) => {
            debug!(%lite, "linking the account's own lite username");
            Ok(Link::LiteUsername(lite.into_bytes()))
        }
        Some(lite) => {
            bail!(
                "the account's lite username {lite:?} is not a linkable `name.NN` label; pass \
             --chat-key <hex 65 bytes> for a standalone registration"
            )
        }
        None => {
            bail!(
                "account has no lite username to link; pass --link-lite <name.NN> or \
             --chat-key <hex 65 bytes> for a standalone registration"
            )
        }
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
