use super::{
    AccountGrant, AuthorityError, BulletinAllowanceKey, CoreStorageKey, HostGrantPersistence,
    StatementStoreAllowanceKey,
};
use crate::platform::normalize_product_identifier;
use crate::runtime::allowances::AllowanceResource;
use parity_scale_codec::DecodeAll;
use zeroize::Zeroizing;

/// Durable cleanup independent of the currently selected wallet or stored values.
#[derive(Clone, PartialEq, Eq)]
pub enum NativeAllowanceDeletion {
    /// Remove grants when a product is uninstalled.
    Product(String),
    /// Remove grants when a wallet is deleted.
    Owner([u8; 32]),
}

impl NativeAllowanceDeletion {
    /// Whether a pending deletion prevents use of this grant.
    pub fn includes(&self, owner: [u8; 32], product_id: &str) -> bool {
        match self {
            Self::Product(product) => product == product_id,
            Self::Owner(root) => *root == owner,
        }
    }

    fn matches(&self, key: &CoreStorageKey) -> bool {
        let CoreStorageKey::NativeAllowanceKey {
            root_public_key,
            product_id,
            ..
        } = key
        else {
            return false;
        };
        match self {
            Self::Product(product) => product_id == product,
            Self::Owner(owner) => root_public_key == owner,
        }
    }
}

fn storage_key(
    owner: [u8; 32],
    product_id: &str,
    resource: AllowanceResource,
) -> Result<CoreStorageKey, AuthorityError> {
    if normalize_product_identifier(product_id).ok().as_deref() != Some(product_id) {
        return Err(AuthorityError::Unavailable {
            reason: "invalid allowance product identifier".to_string(),
        });
    }
    Ok(CoreStorageKey::NativeAllowanceKey {
        root_public_key: owner,
        product_id: product_id.to_string(),
        resource,
    })
}

/// Load only the selected wallet, product and resource, without reading other grants.
pub async fn native_allowance(
    storage: &HostGrantPersistence<'_>,
    owner: [u8; 32],
    product_id: &str,
    resource: AllowanceResource,
) -> Result<Option<AccountGrant>, AuthorityError> {
    let Some(secret) = storage
        .store
        .storage
        .read_core_storage(storage_key(owner, product_id, resource)?)
        .await
        .map_err(storage_error)?
    else {
        return Ok(None);
    };
    let secret = Zeroizing::new(secret);
    match resource {
        AllowanceResource::StatementStore => Ok(Some(AccountGrant::StatementStore {
            key: StatementStoreAllowanceKey::from_secret_bytes(secret.to_vec())?,
            period: None,
        })),
        AllowanceResource::Bulletin => Ok(Some(AccountGrant::Bulletin(
            BulletinAllowanceKey::from_secret_bytes(secret.to_vec())?,
        ))),
    }
}

/// Replace one grant only after the host has validated the issuing activation.
pub async fn retain_native_allowance(
    storage: &HostGrantPersistence<'_>,
    session_state: &crate::host_logic::session::SessionState,
    session: &crate::host_logic::session::SessionInfo,
    revision: u64,
    product_id: &str,
    allowance: &AccountGrant,
) -> Result<(), AuthorityError> {
    let (resource, secret) = match allowance {
        AccountGrant::StatementStore { key, .. } => {
            (AllowanceResource::StatementStore, key.as_secret_bytes())
        }
        AccountGrant::Bulletin(key) => (AllowanceResource::Bulletin, key.as_secret_bytes()),
        _ => {
            return Err(AuthorityError::Unavailable {
                reason: "expected an allowance grant".to_string(),
            });
        }
    };
    let key = storage_key(session.public_key, product_id, resource)?;
    if !storage
        .store
        .session_secret_allocation_is_current(session_state, session, revision)
    {
        return Err(AuthorityError::Disconnected);
    }
    storage
        .store
        .storage
        .write_core_storage(key, secret.to_vec())
        .await
        .map_err(storage_error)
}

/// Clear matching slots even when their values cannot be decoded or decrypted.
pub async fn delete_native_allowances(
    storage: &HostGrantPersistence<'_>,
    deletion: &NativeAllowanceDeletion,
) -> Result<(), AuthorityError> {
    let keys = storage
        .store
        .storage
        .core_storage_keys()
        .await
        .map_err(storage_error)?;
    let mut first_error = None;
    for encoded in keys.encoded_keys {
        let Ok(key) = CoreStorageKey::decode_all(&mut encoded.as_slice()) else {
            continue;
        };
        if deletion.matches(&key)
            && let Err(error) = storage.store.storage.clear_core_storage(key).await
            && first_error.is_none()
        {
            first_error = Some(storage_error(error));
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn storage_error(error: crate::latest::GenericError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: error.reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CoreStorage;
    use super::super::HostGrantStore;
    use crate::runtime::authority::GrantKeeping;
    use crate::test_support::StubPlatform;
    use futures::executor::block_on;
    use std::sync::Arc;

    #[test]
    fn corrupt_grants_are_isolated_and_removable_by_product_or_owner() {
        block_on(async {
            let platform = Arc::new(StubPlatform::default());
            let store = HostGrantStore::new(platform.clone(), GrantKeeping::Wallet);
            let guard = store.persistence().await;
            let secret = crate::host_logic::product_account::derive_sr25519_hard_path(
                &[7; 16],
                &["allowance", "bulletin"],
            )
            .unwrap()
            .secret
            .to_bytes()
            .to_vec();
            let broken =
                storage_key([7; 32], "myapp.dot", AllowanceResource::StatementStore).unwrap();
            let other_product =
                storage_key([7; 32], "other.dot", AllowanceResource::Bulletin).unwrap();
            let other_wallet =
                storage_key([8; 32], "other.dot", AllowanceResource::Bulletin).unwrap();
            platform
                .write_core_storage(broken.clone(), vec![0xff])
                .await
                .unwrap();
            for key in [&other_product, &other_wallet] {
                platform
                    .write_core_storage(key.clone(), secret.clone())
                    .await
                    .unwrap();
            }
            assert!(
                native_allowance(
                    &guard,
                    [7; 32],
                    "myapp.dot",
                    AllowanceResource::StatementStore
                )
                .await
                .is_err()
            );
            assert!(matches!(
                native_allowance(&guard, [7; 32], "other.dot", AllowanceResource::Bulletin)
                    .await
                    .unwrap(),
                Some(AccountGrant::Bulletin(_))
            ));
            delete_native_allowances(
                &guard,
                &NativeAllowanceDeletion::Product("myapp.dot".to_string()),
            )
            .await
            .unwrap();
            assert_eq!(platform.read_core_storage(broken).await.unwrap(), None);
            delete_native_allowances(&guard, &NativeAllowanceDeletion::Owner([7; 32]))
                .await
                .unwrap();
            assert_eq!(
                (
                    platform.read_core_storage(other_product).await.unwrap(),
                    platform.read_core_storage(other_wallet).await.unwrap()
                ),
                (None, Some(secret.clone()))
            );
        });
    }
}
