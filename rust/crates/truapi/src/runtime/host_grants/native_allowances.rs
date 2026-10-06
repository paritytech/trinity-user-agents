use super::{
    AccountGrant, AuthorityError, BulletinAllowanceKey, HostGrantPersistence, SecretCoreStorageKey,
    StatementStoreAllowanceKey,
};
use crate::platform::normalize_product_identifier;
use crate::runtime::allowances::AllowanceResource;
use parity_scale_codec::{Decode, Encode};
use std::collections::HashSet;
use zeroize::{Zeroize, Zeroizing};

#[derive(Encode, Decode)]
enum StoredAllowances {
    #[codec(index = 1)]
    V1(Vec<StoredAllowance>),
}

#[derive(Encode, Decode, zeroize::ZeroizeOnDrop)]
struct StoredAllowance {
    #[zeroize(skip)]
    owner: [u8; 32],
    #[zeroize(skip)]
    product_id: String,
    grant: StoredGrant,
}

#[derive(Encode, Decode, Zeroize, zeroize::ZeroizeOnDrop)]
enum StoredGrant {
    #[codec(index = 0)]
    StatementStore {
        #[zeroize(skip)]
        period: u32,
        secret: [u8; 64],
    },
    #[codec(index = 1)]
    Bulletin { secret: [u8; 64] },
}

impl StoredGrant {
    fn resource(&self) -> AllowanceResource {
        match self {
            Self::StatementStore { .. } => AllowanceResource::StatementStore,
            Self::Bulletin { .. } => AllowanceResource::Bulletin,
        }
    }

    fn allowance(&self) -> Result<AccountGrant, AuthorityError> {
        match self {
            Self::StatementStore { period, secret } => Ok(AccountGrant::StatementStore {
                key: StatementStoreAllowanceKey::from_secret_bytes(secret.to_vec())?,
                period: Some(*period),
            }),
            Self::Bulletin { secret } => Ok(AccountGrant::Bulletin(
                BulletinAllowanceKey::from_secret_bytes(secret.to_vec())?,
            )),
        }
    }
}

/// Durable cleanup scopes, independent of the currently selected wallet.
#[derive(Clone, PartialEq, Eq)]
pub enum NativeAllowanceDeletion {
    /// Product reset includes every retained owner.
    Product(String),
    /// Refresh discards this owner's Bulletin key before issuing another.
    Bulletin { owner: [u8; 32], product_id: String },
    /// A rejected statement must not evict a replacement key.
    StatementStore {
        owner: [u8; 32],
        product_id: String,
        public_key: [u8; 32],
        period: u32,
    },
}

impl NativeAllowanceDeletion {
    fn matches(&self, entry: &StoredAllowance) -> Result<bool, AuthorityError> {
        match self {
            Self::Product(product_id) => Ok(entry.product_id == *product_id),
            Self::Bulletin { owner, product_id } => Ok(entry.owner == *owner
                && entry.product_id == *product_id
                && matches!(entry.grant, StoredGrant::Bulletin { .. })),
            Self::StatementStore {
                owner,
                product_id,
                public_key,
                period,
            } => {
                if entry.owner != *owner || entry.product_id != *product_id {
                    return Ok(false);
                }
                Ok(
                    matches!(entry.grant.allowance()?, AccountGrant::StatementStore { key, period: Some(stored_period) } if key.public_key == *public_key && stored_period == *period),
                )
            }
        }
    }
}

impl HostGrantPersistence<'_> {
    async fn read_native_allowances(&self) -> Result<Vec<StoredAllowance>, AuthorityError> {
        let Some(blob) = self
            .store
            .secret_storage
            .read_secret_core_storage(SecretCoreStorageKey::NativeAllowanceKeys)
            .await
            .map_err(storage_error)?
        else {
            return Ok(Vec::new());
        };
        let blob = Zeroizing::new(blob);
        let mut input = blob.as_slice();
        let StoredAllowances::V1(entries) =
            StoredAllowances::decode(&mut input).map_err(|_| invalid_records())?;
        if !input.is_empty() {
            return Err(invalid_records());
        }
        let mut identities = HashSet::new();
        for entry in &entries {
            if normalize_product_identifier(&entry.product_id).map_err(|_| invalid_records())?
                != entry.product_id
                || !identities.insert((entry.owner, &entry.product_id, entry.grant.resource()))
            {
                return Err(invalid_records());
            }
            entry.grant.allowance()?;
        }
        Ok(entries)
    }

    async fn write_native_allowances(
        &self,
        entries: Vec<StoredAllowance>,
    ) -> Result<(), AuthorityError> {
        if entries.is_empty() {
            self.store
                .secret_storage
                .clear_secret_core_storage(SecretCoreStorageKey::NativeAllowanceKeys)
                .await
                .map_err(storage_error)
        } else {
            self.store
                .secret_storage
                .write_secret_core_storage(
                    SecretCoreStorageKey::NativeAllowanceKeys,
                    StoredAllowances::V1(entries).encode(),
                )
                .await
                .map_err(storage_error)
        }
    }

    /// Load only the stable wallet's validated resource grant.
    pub async fn native_allowance(
        &self,
        owner: [u8; 32],
        product_id: &str,
        resource: AllowanceResource,
    ) -> Result<Option<AccountGrant>, AuthorityError> {
        self.read_native_allowances()
            .await?
            .iter()
            .find(|entry| {
                entry.owner == owner
                    && entry.product_id == product_id
                    && entry.grant.resource() == resource
            })
            .map(|entry| entry.grant.allowance())
            .transpose()
    }

    /// Replace one resource, retaining the other owners and products.
    pub async fn retain_native_allowance(
        &self,
        session_state: &crate::host_logic::session::SessionState,
        session: &crate::host_logic::session::SessionInfo,
        revision: u64,
        product_id: &str,
        allowance: &AccountGrant,
    ) -> Result<(), AuthorityError> {
        let grant = match allowance {
            AccountGrant::StatementStore {
                key,
                period: Some(period),
            } => StoredGrant::StatementStore {
                period: *period,
                secret: key.secret,
            },
            AccountGrant::Bulletin(key) => StoredGrant::Bulletin {
                secret: *key.as_secret_bytes(),
            },
            _ => {
                return Err(AuthorityError::Unavailable {
                    reason: "native allowance requires its resource and allocation period"
                        .to_string(),
                });
            }
        };
        if normalize_product_identifier(product_id).map_err(|_| invalid_records())? != product_id {
            return Err(invalid_records());
        }
        grant.allowance()?;
        let mut entries = self.read_native_allowances().await?;
        if !self
            .store
            .session_secret_allocation_is_current(session_state, session, revision)
        {
            return Err(AuthorityError::Disconnected);
        }
        let owner = session.public_key;
        entries.retain(|entry| {
            entry.owner != owner
                || entry.product_id != product_id
                || entry.grant.resource() != grant.resource()
        });
        entries.push(StoredAllowance {
            owner,
            product_id: product_id.to_string(),
            grant,
        });
        self.write_native_allowances(entries).await
    }

    /// Apply a queued scope without destroying unrelated records on decode failure.
    pub async fn delete_native_allowances(
        &self,
        deletion: &NativeAllowanceDeletion,
    ) -> Result<(), AuthorityError> {
        let entries = self.read_native_allowances().await?;
        let before = entries.len();
        let mut retained = Vec::with_capacity(before);
        for entry in entries {
            if !deletion.matches(&entry)? {
                retained.push(entry);
            }
        }
        if retained.len() != before {
            self.write_native_allowances(retained).await?;
        }
        Ok(())
    }
}

fn invalid_records() -> AuthorityError {
    AuthorityError::Unavailable {
        reason: "persisted native allowances are invalid".to_string(),
    }
}

fn storage_error(error: crate::latest::GenericError) -> AuthorityError {
    AuthorityError::Unavailable {
        reason: error.reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::SecretCoreStorage;
    use crate::runtime::HostGrantStore;
    use crate::test_support::StubPlatform;
    use futures::executor::block_on;
    use std::sync::Arc;

    fn record(product: &str, secret: [u8; 64]) -> StoredAllowance {
        StoredAllowance {
            owner: [7; 32],
            product_id: product.to_string(),
            grant: StoredGrant::StatementStore { period: 3, secret },
        }
    }

    #[test]
    fn native_decode_rejects_ambiguous_or_invalid_records_without_rewriting_them() {
        let platform = Arc::new(StubPlatform::default());
        let store = HostGrantStore::new(platform.clone(), platform.clone());
        let guard = block_on(store.persistence());
        let valid = StoredAllowances::V1(vec![record("myapp.dot", [0x42; 64])]).encode();
        block_on(
            platform.write_secret_core_storage(
                SecretCoreStorageKey::NativeAllowanceKeys,
                valid.clone(),
            ),
        )
        .unwrap();
        assert!(matches!(
            block_on(guard.native_allowance(
                [7; 32],
                "myapp.dot",
                AllowanceResource::StatementStore
            ))
            .unwrap(),
            Some(AccountGrant::StatementStore {
                period: Some(3),
                ..
            })
        ));
        let mut trailing = valid;
        trailing.push(0);
        for blob in [
            vec![0xff],
            trailing,
            StoredAllowances::V1(vec![
                record("myapp.dot", [0x42; 64]),
                record("myapp.dot", [0x42; 64]),
            ])
            .encode(),
            StoredAllowances::V1(vec![record("https://myapp.dot/path", [0x42; 64])]).encode(),
            StoredAllowances::V1(vec![record("myapp.dot", [0xff; 64])]).encode(),
        ] {
            block_on(platform.write_secret_core_storage(
                SecretCoreStorageKey::NativeAllowanceKeys,
                blob.clone(),
            ))
            .unwrap();
            let loaded = block_on(guard.native_allowance(
                [7; 32],
                "myapp.dot",
                AllowanceResource::StatementStore,
            ));
            let retained = block_on(
                platform.read_secret_core_storage(SecretCoreStorageKey::NativeAllowanceKeys),
            )
            .unwrap();
            assert_eq!((loaded.is_err(), retained), (true, Some(blob)));
        }
    }
}
