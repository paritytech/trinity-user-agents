// SPDX-License-Identifier: AGPL-3.0-only
// Derived from polkavm-app-kit polkavm-chat-v2/src/recipient.rs and Brevity's
// brevity-ffi/src/handle.rs peer resolution. Copyright their contributors.

//! Host-authenticated public identity. People owns encryption keys; Asset Hub
//! dotNS owns names. Neither a product nor a peer supplies either association.

mod dotns;
mod rpc;
mod schema;

use super::NativeChatContext;
use truapi::latest::HostProductDeviceChatError as Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedPeer {
    pub(crate) identity_account_id: [u8; 32],
    pub(crate) chat_public_key: [u8; 32],
    pub(crate) username: Option<String>,
}

pub(crate) async fn resolve_username(
    context: &NativeChatContext,
    username: &str,
) -> Result<ResolvedPeer, Error> {
    ensure_session(context)?;
    let username = normalize_username(username)?;
    let mut directory = dotns::Directory::open(context)
        .await
        .map_err(directory_error)?;
    let owner = directory
        .owner(&username)
        .await
        .map_err(directory_error)?
        .ok_or(Error::RecipientNotFound)?;
    if let Some(account) = owner.account {
        match people_key(context, account).await {
            Ok(chat_public_key) => {
                return Ok(ResolvedPeer {
                    identity_account_id: account,
                    chat_public_key,
                    username: Some(username),
                });
            }
            Err(Error::RecipientNotFound) if owner.is_unmapped_hint() => {}
            Err(error) => return Err(error),
        }
    }
    let account = username_candidate(context, &username, &owner.address).await?;
    let chat_public_key = people_key(context, account).await?;
    ensure_session(context)?;
    Ok(ResolvedPeer {
        identity_account_id: account,
        chat_public_key,
        username: Some(username),
    })
}

pub(crate) async fn resolve_account(
    context: &NativeChatContext,
    account: [u8; 32],
) -> Result<ResolvedPeer, Error> {
    let chat_public_key = people_key(context, account).await?;
    // A directory outage must not turn a chain-authenticated incoming identity
    // into an arbitrary-key fallback or make its independent People key unusable.
    let username = match dotns::verified_label(context, &account).await {
        Ok(username) => username,
        Err(reason) => {
            tracing::debug!(%reason, "native Chat peer name unavailable");
            None
        }
    };
    ensure_session(context)?;
    Ok(ResolvedPeer {
        identity_account_id: account,
        chat_public_key,
        username,
    })
}

async fn username_candidate(
    context: &NativeChatContext,
    username: &str,
    owner: &[u8; 20],
) -> Result<[u8; 32], Error> {
    ensure_session(context)?;
    let snapshot = rpc::Snapshot::open(context, context.genesis_hash)
        .await
        .map_err(network_error)?;
    let metadata = snapshot.metadata().await.map_err(network_error)?;
    if let Some(schema) = schema::UsernameOwnerSchema::new(&metadata).map_err(directory_error)? {
        let key = schema.key(username).map_err(directory_error)?;
        if let Some(encoded) = snapshot.storage_value(&key).await.map_err(network_error)? {
            let candidate: [u8; 32] = encoded
                .as_slice()
                .try_into()
                .map_err(|_| directory_error("legacy username owner is not AccountId32".into()))?;
            if let Some(account) =
                dotns::verified_candidate(&[candidate], owner).map_err(directory_error)?
            {
                return Ok(account);
            }
        }
    }
    ensure_session(context)?;
    let backend = context
        .services
        .identity_backend_host()
        .ok_or(Error::NetworkUnavailable)?;
    let candidates = backend
        .identity_username_candidates(username.to_owned(), context.genesis_hash)
        .await
        .map_err(|_| Error::NetworkUnavailable)?;
    ensure_session(context)?;
    // The registry already proves this name exists. An unavailable, stale or
    // incomplete index is not evidence that its recipient is absent.
    dotns::verified_candidate(&candidates, owner)
        .map_err(directory_error)?
        .ok_or(Error::NetworkUnavailable)
}

async fn people_key(context: &NativeChatContext, account: [u8; 32]) -> Result<[u8; 32], Error> {
    ensure_session(context)?;
    if account == [0; 32] {
        return Err(Error::InvalidRequest);
    }
    if context.genesis_hash != context.services.people_chain_genesis_hash {
        return Err(Error::NetworkUnavailable);
    }
    let snapshot = rpc::Snapshot::open(context, context.genesis_hash)
        .await
        .map_err(network_error)?;
    let metadata = snapshot.metadata().await.map_err(network_error)?;
    let schema = schema::ConsumerSchema::new(&metadata).map_err(directory_error)?;
    let key = schema.key(&account);
    let value = snapshot
        .storage_value(&key)
        .await
        .map_err(network_error)?
        .ok_or(Error::RecipientNotFound)?;
    let public_key = schema.identifier(&value).map_err(directory_error)?;
    ensure_session(context)?;
    Ok(public_key)
}

fn ensure_session(context: &NativeChatContext) -> Result<(), Error> {
    if (context.session_valid)() {
        Ok(())
    } else {
        Err(Error::NotConnected)
    }
}

fn network_error(reason: String) -> Error {
    tracing::debug!(%reason, "native Chat configured chain unavailable");
    Error::NetworkUnavailable
}

fn directory_error(reason: String) -> Error {
    tracing::debug!(%reason, "native Chat configured identity directory unavailable");
    Error::NetworkUnavailable
}

fn normalize_username(username: &str) -> Result<String, Error> {
    // Exact-key semantics of the deployed guest and host identity encoding:
    // ASCII case folding only; never rewrite suffix digits or resolve a prefix.
    let username = username.trim();
    if username.is_empty() || username.len() > 32 {
        return Err(Error::InvalidRequest);
    }
    let username = username.to_ascii_lowercase();
    if username
        .bytes()
        .any(|byte| !matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-'))
        || username.starts_with('.')
        || username.ends_with('.')
        || username.contains("..")
    {
        return Err(Error::InvalidRequest);
    }
    Ok(username)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_username_normalization_never_rewrites_numeric_suffixes() {
        assert_eq!(normalize_username("  ALICE.01 ").unwrap(), "alice.01");
        assert_eq!(normalize_username("alice.1").unwrap(), "alice.1");
        assert_ne!(
            normalize_username("alice.01"),
            normalize_username("alice.1")
        );
        assert_eq!(
            normalize_username("  ALICE-JANE.0123 ").unwrap(),
            "alice-jane.0123"
        );
        for name in [
            "",
            "álice",
            ".alice",
            "alice.",
            "alice..01",
            "alice\0",
            "alice/01",
        ] {
            assert!(normalize_username(name).is_err());
        }
        assert!(normalize_username(&"a".repeat(33)).is_err());
    }
}
