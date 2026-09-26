//! Profile disclosure state: the reference the user disclosed to their chat
//! contacts, and the references this product's contacts disclosed to them.
//!
//! Both are bearer capabilities. They live in core storage, never in product
//! storage, and never cross back to a product: `present_contact` names a
//! contact and the host substitutes the reference.

use parity_scale_codec::{Decode, Encode};
use truapi_platform::{CoreStorage, CoreStorageKey};

/// The user's own disclosed reference and the product that disclosed it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct Disclosure {
    pub(crate) product_id: String,
    pub(crate) reference: String,
}

/// One contact's disclosed reference, as their host sent it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct ReceivedReference {
    pub(crate) peer_identity: [u8; 32],
    /// The product on the contact's side that disclosed it.
    pub(crate) discloser_product_id: String,
    pub(crate) reference: String,
}

/// Versioned so the slot can change shape without a silent misread.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
enum StoredReferences {
    #[codec(index = 0)]
    V1(Vec<ReceivedReference>),
}

/// A contact roster is bounded; so is what the host keeps for it.
const MAX_RECEIVED_REFERENCES: usize = 4096;

fn storage_error(error: impl core::fmt::Debug) -> String {
    format!("profile storage failed: {error:?}")
}

pub(crate) async fn read_disclosure(
    storage: &(impl CoreStorage + ?Sized),
) -> Result<Option<Disclosure>, String> {
    let Some(raw) = storage
        .read_core_storage(CoreStorageKey::ProfileDisclosure)
        .await
        .map_err(storage_error)?
    else {
        return Ok(None);
    };
    Disclosure::decode(&mut raw.as_slice())
        .map(Some)
        .map_err(|error| format!("stored profile disclosure is unreadable: {error}"))
}

pub(crate) async fn write_disclosure(
    storage: &(impl CoreStorage + ?Sized),
    disclosure: &Disclosure,
) -> Result<(), String> {
    storage
        .write_core_storage(CoreStorageKey::ProfileDisclosure, disclosure.encode())
        .await
        .map_err(storage_error)
}

pub(crate) async fn clear_disclosure(storage: &(impl CoreStorage + ?Sized)) -> Result<(), String> {
    storage
        .clear_core_storage(CoreStorageKey::ProfileDisclosure)
        .await
        .map_err(storage_error)
}

async fn read_received(
    storage: &(impl CoreStorage + ?Sized),
    product_id: &str,
) -> Result<Vec<ReceivedReference>, String> {
    let key = CoreStorageKey::ProfileReferencesReceived {
        product_id: product_id.to_string(),
    };
    let Some(raw) = storage
        .read_core_storage(key)
        .await
        .map_err(storage_error)?
    else {
        return Ok(Vec::new());
    };
    match StoredReferences::decode(&mut raw.as_slice()) {
        Ok(StoredReferences::V1(entries)) => Ok(entries),
        Err(error) => Err(format!("stored profile references are unreadable: {error}")),
    }
}

/// The reference a contact disclosed to this product's user, if any.
pub(crate) async fn received_reference(
    storage: &(impl CoreStorage + ?Sized),
    product_id: &str,
    peer_identity: &[u8; 32],
) -> Result<Option<ReceivedReference>, String> {
    Ok(read_received(storage, product_id)
        .await?
        .into_iter()
        .find(|entry| &entry.peer_identity == peer_identity))
}

/// Record what a contact's host sent: the newest reference replaces the old
/// one, and `None` (a retraction) removes it.
pub(crate) async fn record_received_reference(
    storage: &(impl CoreStorage + ?Sized),
    product_id: &str,
    peer_identity: [u8; 32],
    discloser_product_id: String,
    reference: Option<String>,
) -> Result<(), String> {
    let mut entries = read_received(storage, product_id).await?;
    entries.retain(|entry| entry.peer_identity != peer_identity);
    if let Some(reference) = reference {
        if entries.len() >= MAX_RECEIVED_REFERENCES {
            return Err("too many contact profile references".to_string());
        }
        entries.push(ReceivedReference {
            peer_identity,
            discloser_product_id,
            reference,
        });
    }
    let key = CoreStorageKey::ProfileReferencesReceived {
        product_id: product_id.to_string(),
    };
    if entries.is_empty() {
        storage.clear_core_storage(key).await.map_err(storage_error)
    } else {
        storage
            .write_core_storage(key, StoredReferences::V1(entries).encode())
            .await
            .map_err(storage_error)
    }
}
