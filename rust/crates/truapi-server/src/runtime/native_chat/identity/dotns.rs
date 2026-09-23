// SPDX-License-Identifier: AGPL-3.0-only
// Adapted from Brevity's dotns_identity.rs and Host dotns_gateway.rs.

//! Resolve the configured Asset Hub's on-chain PoP directory, never a guessed
//! backend URL. Protocol components and TLD are discovered from the gateway at
//! one finalized pin. The registry, not the append-only LabelStore, owns names.

use super::{NativeChatContext, normalize_username, rpc::Snapshot, schema};
#[cfg(test)]
use crate::host_logic::dotns_gateway as gateway;
use crate::host_logic::dotns_gateway::{
    DotnsTransport, account_to_h160, call_bytes32, call_no_args, decode_address, decode_bool,
    discover_pop_controller, is_dns_label, is_pop_issued, namehash_under, network_tld,
    protocol_component, resolve_labels, tld_node,
};

pub(super) struct Directory {
    snapshot: Snapshot,
    controller: [u8; 20],
    registry: [u8; 20],
    tld: [u8; 32],
    schema: schema::GatewaySchema,
}

pub(super) struct Owner {
    pub(super) account: Option<[u8; 32]>,
    pub(super) address: [u8; 20],
}

impl Owner {
    pub(super) fn is_unmapped_hint(&self) -> bool {
        self.account
            .is_none_or(|account| account[20..] == [0xee; 12])
    }
}

impl Directory {
    pub(super) async fn open(context: &NativeChatContext) -> Result<Self, String> {
        let genesis = context
            .services
            .asset_hub_chain_genesis_hash()
            .ok_or("current Host network has no Asset Hub directory")?;
        let mut snapshot = Snapshot::open(context, genesis).await?;
        let schema = schema::validate_gateway(&snapshot.metadata().await?)?;
        let controller = discover_pop_controller(&mut snapshot)
            .await?
            .filter(|address| *address != [0; 20])
            .ok_or("current Host network has no dotNS PoP controller")?;
        let output = snapshot
            .view(&controller, call_no_args("protocolRegistry()"))
            .await?;
        let protocol = address(&output)?;
        if protocol == [0; 20] {
            return Err("dotNS protocol registry is unconfigured".into());
        }
        let tld = network_tld(&mut snapshot, &protocol).await?;
        // The Host's configured suffix is a consistency check, never permission
        // to try another chain, contract or identity backend when lookup fails.
        let suffix = tld
            .strip_prefix('.')
            .ok_or("dotNS TLD has no leading dot")?;
        if suffix != context.network_suffix || !is_dns_label(suffix) {
            return Err("dotNS TLD differs from current Host network".into());
        }
        let registry = protocol_component(&mut snapshot, &protocol, "registry").await?;
        if registry == [0; 20] {
            return Err("dotNS name registry is unconfigured".into());
        }
        Ok(Self {
            snapshot,
            controller,
            registry,
            tld: tld_node(&tld),
            schema,
        })
    }

    pub(super) async fn owner(&mut self, username: &str) -> Result<Option<Owner>, String> {
        let owner = forward_owner(
            &mut self.snapshot,
            &self.controller,
            &self.registry,
            &self.tld,
            username,
        )
        .await?;
        let Some(owner) = owner else {
            return Ok(None);
        };
        // Gateway-issued lite names retain the original AccountId32 even before
        // the person maps a Revive address. This is a metadata-directed exact
        // lookup, and still must agree with the live forward registry owner.
        if is_lite_username(username)
            && let Some(key) = self.schema.lite_owner_key(username)?
            && let Some(encoded) = self.snapshot.storage_value(&key).await?
        {
            return Ok(Some(Owner {
                account: Some(account_for_owner(&encoded, &owner)?),
                address: owner,
            }));
        }
        // ReviveApi_account_id is the canonical mapped H160 -> AccountId32
        // conversion used by dotns-cli's get_substrate_address. Never pad a
        // contract owner or borrow the caller's claimed account as a substitute.
        let account = match self
            .snapshot
            .runtime_call("ReviveApi_account_id", &owner)
            .await
        {
            Ok(encoded) => Some(account_for_owner(&encoded, &owner)?),
            Err(reason) => {
                // Some deployments cannot invert an unmapped H160. A native
                // index may supply a candidate, never replace the owner above.
                tracing::debug!(%reason, "dotNS inverse account lookup unavailable");
                None
            }
        };
        Ok(Some(Owner {
            account,
            address: owner,
        }))
    }

    pub(super) async fn labels(&mut self, account: &[u8; 32]) -> Result<Vec<String>, String> {
        let labels = resolve_labels(&mut self.snapshot, &self.controller, account).await?;
        Ok(labels
            .into_iter()
            .filter(|label| {
                normalize_username(label)
                    .as_ref()
                    .is_ok_and(|normalized| normalized == label)
                    && (is_dns_label(label) || is_lite_username(label))
            })
            .collect())
    }
}

pub(super) async fn verified_label(
    context: &NativeChatContext,
    account: &[u8; 32],
) -> Result<Option<String>, String> {
    let mut directory = Directory::open(context).await?;
    let labels = directory.labels(account).await?;
    // Native preference is full then lite. Check EVERY candidate forwards:
    // transferred-away names survive forever in the append-only LabelStore.
    for lite in [false, true] {
        for label in labels
            .iter()
            .filter(|label| is_lite_username(label) == lite)
        {
            let owner = forward_owner(
                &mut directory.snapshot,
                &directory.controller,
                &directory.registry,
                &directory.tld,
                label,
            )
            .await?;
            if owner == Some(account_to_h160(account)) {
                return Ok(Some(label.clone()));
            }
        }
    }
    Ok(None)
}

fn is_lite_username(label: &str) -> bool {
    // Lookup is not registration: preserve every already-issued numeric suffix
    // verbatim, including deployments with longer suffixes and DNS stems.
    // isPopIssued below is the authority, not today's registration shape rules.
    label.len() <= 32
        && label.split_once('.').is_some_and(|(stem, suffix)| {
            is_dns_label(stem)
                && !suffix.is_empty()
                && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn account_for_owner(encoded: &[u8], owner: &[u8; 20]) -> Result<[u8; 32], String> {
    let account: [u8; 32] = encoded
        .try_into()
        .map_err(|_| "dotNS owner runtime API did not return AccountId32")?;
    if account == [0; 32] || account_to_h160(&account) != *owner {
        return Err("dotNS owner account does not map back to authoritative H160".into());
    }
    Ok(account)
}

pub(super) fn verified_candidate(
    candidates: &[[u8; 32]],
    owner: &[u8; 20],
) -> Result<Option<[u8; 32]>, String> {
    if candidates.len() > 32 {
        return Err("too many username candidates".into());
    }
    let mut verified = None;
    for account in candidates {
        if *account == [0; 32] {
            return Err("zero username candidate".into());
        }
        if account_to_h160(account) != *owner {
            continue;
        }
        if verified.replace(*account).is_some() {
            return Err("ambiguous username candidates".into());
        }
    }
    Ok(verified)
}

/// Both deployed representations are chain-visible: earlier contracts mint a
/// dotted lite label as one second-level token; current DotnsPopController
/// registers it as a subnode beneath its numeric suffix. Read the registry's
/// owner (which itself delegates tokenized nodes to registrar.ownerOf). Never
/// flatten a suffix or choose between conflicting owners of the two spellings.
async fn forward_owner<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    controller: &[u8; 20],
    registry: &[u8; 20],
    tld: &[u8; 32],
    username: &str,
) -> Result<Option<[u8; 20]>, String> {
    if !(is_dns_label(username) || is_lite_username(username))
        || !is_pop_issued(transport, controller, username).await?
    {
        return Ok(None);
    }
    let atomic_node = namehash_under(tld, username);
    let atomic = node_owner(transport, registry, &atomic_node).await?;
    if !is_lite_username(username) {
        return Ok(atomic);
    }
    let (stem, suffix) = username
        .split_once('.')
        .ok_or("invalid dotted lite label")?;
    let subnode = namehash_under(&namehash_under(tld, suffix), stem);
    let nested = node_owner(transport, registry, &subnode).await?;
    match (atomic, nested) {
        (Some(a), Some(b)) if a != b => Err("ambiguous dotNS lite name owner".into()),
        (Some(owner), _) | (_, Some(owner)) => Ok(Some(owner)),
        (None, None) => Ok(None),
    }
}

async fn node_owner<T: DotnsTransport + ?Sized>(
    transport: &mut T,
    registry: &[u8; 20],
    node: &[u8; 32],
) -> Result<Option<[u8; 20]>, String> {
    let exists = transport
        .view(registry, call_bytes32("recordExists(bytes32)", node))
        .await?;
    if exists.len() != 32 {
        return Err("dotNS recordExists returned a non-word".into());
    }
    if !decode_bool(&exists).map_err(|error| error.to_string())? {
        return Ok(None);
    }
    let owner = address(
        &transport
            .view(registry, call_bytes32("owner(bytes32)", node))
            .await?,
    )?;
    if owner == [0; 20] {
        return Err("existing dotNS record has a zero owner".into());
    }
    Ok(Some(owner))
}

fn address(bytes: &[u8]) -> Result<[u8; 20], String> {
    if bytes.len() != 32 {
        return Err("dotNS address is not one ABI word".into());
    }
    decode_address(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gateway::DotnsViewError;
    use std::collections::HashMap;

    struct Registry {
        issued: bool,
        owners: HashMap<[u8; 32], [u8; 20]>,
    }

    #[truapi_platform::async_trait]
    impl DotnsTransport for Registry {
        async fn storage(&mut self, _: Vec<u8>) -> Result<Option<Vec<u8>>, String> {
            unreachable!()
        }
        async fn view(&mut self, _: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, DotnsViewError> {
            let mut word = vec![0; 32];
            if input[..4] == gateway::selector("isPopIssued(string)") {
                word[31] = self.issued as u8;
            } else {
                let node: [u8; 32] = input[4..].try_into().unwrap();
                if input[..4] == gateway::selector("recordExists(bytes32)") {
                    word[31] = self.owners.contains_key(&node) as u8;
                } else {
                    assert_eq!(input[..4], gateway::selector("owner(bytes32)"));
                    word[12..].copy_from_slice(self.owners.get(&node).unwrap());
                }
            }
            Ok(word)
        }
    }

    #[test]
    fn supports_both_deployed_lite_nodes_without_accepting_conflicting_owners() {
        futures::executor::block_on(async {
            let tld = tld_node(".paseo");
            let atomic = namehash_under(&tld, "alice.42");
            let nested = namehash_under(&namehash_under(&tld, "42"), "alice");
            let mut registry = Registry {
                issued: true,
                owners: HashMap::new(),
            };
            registry.owners.insert(atomic, [1; 20]);
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice.42")
                    .await
                    .unwrap(),
                Some([1; 20])
            );
            registry.owners.clear();
            registry.owners.insert(nested, [4; 20]);
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice.42")
                    .await
                    .unwrap(),
                Some([4; 20])
            );
            registry.owners.insert(atomic, [1; 20]);
            assert!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice.42")
                    .await
                    .is_err()
            );
            registry.issued = false;
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice.42")
                    .await
                    .unwrap(),
                None
            );
        });
    }

    #[test]
    fn forward_lookup_preserves_dns_stems_and_exact_long_numeric_suffixes() {
        futures::executor::block_on(async {
            let tld = tld_node(".paseo");
            let node = namehash_under(&namehash_under(&tld, "0123"), "alice-jane");
            let mut registry = Registry {
                issued: true,
                owners: HashMap::from([(node, [7; 20])]),
            };
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice-jane.0123")
                    .await
                    .unwrap(),
                Some([7; 20])
            );
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alice-jane.123")
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                forward_owner(&mut registry, &[2; 20], &[3; 20], &tld, "alicejane.0123")
                    .await
                    .unwrap(),
                None
            );
        });
    }

    #[test]
    fn mapped_owner_cannot_be_replaced_by_claimed_account_or_alias() {
        let account = [3; 32];
        let owner = account_to_h160(&account);
        assert_eq!(account_for_owner(&account, &owner).unwrap(), account);
        assert!(account_for_owner(&[4; 32], &owner).is_err());
        assert!(account_for_owner(&[3; 31], &owner).is_err());
        let mut padded = [0xee; 32];
        padded[..20].copy_from_slice(&[5; 20]);
        assert_eq!(account_for_owner(&padded, &[5; 20]).unwrap(), padded);
    }

    #[test]
    fn backend_candidates_never_override_chain_owner_or_choose_an_ambiguous_match() {
        let account = [3; 32];
        let owner = account_to_h160(&account);
        assert_eq!(
            verified_candidate(&[account], &owner).unwrap(),
            Some(account)
        );
        assert_eq!(verified_candidate(&[[4; 32]], &owner).unwrap(), None);
        assert!(verified_candidate(&[account, account], &owner).is_err());
        assert!(verified_candidate(&[[0; 32]], &owner).is_err());
        assert!(verified_candidate(&[account; 33], &owner).is_err());
        // Revive's inverse fallback shares the H160 but is not evidence of the
        // sr25519 identity's People key. Two matching accounts are ambiguous.
        let mut padded = [0xee; 32];
        padded[..20].copy_from_slice(&owner);
        assert!(verified_candidate(&[account, padded], &owner).is_err());
    }
}
