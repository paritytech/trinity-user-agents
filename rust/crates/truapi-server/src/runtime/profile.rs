//! Profile disclosure state: the reference the user disclosed to their chat
//! contacts, and the references their contacts disclosed to them.
//!
//! Both are bearer capabilities. They live in core storage, never in product
//! storage, and never cross back to a product: `present_contact` and placed
//! contact avatars name a contact and the host substitutes the reference. Both
//! belong to one wallet on one Chat network, like the roster they travel over.

pub(crate) mod avatars;

use parity_scale_codec::{Decode, Encode};
use truapi_platform::{CoreStorage, CoreStorageKey};

/// The wallet and Chat network a disclosure, and what contacts sent back,
/// belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfileOwner {
    /// Root public key of the wallet.
    pub(crate) root_public_key: [u8; 32],
    /// Host-selected Chat network.
    pub(crate) genesis_hash: [u8; 32],
}

impl ProfileOwner {
    fn disclosure_key(&self) -> CoreStorageKey {
        CoreStorageKey::ProfileDisclosure {
            root_public_key: self.root_public_key,
            genesis_hash: self.genesis_hash,
        }
    }

    fn received_key(&self, product_id: &str) -> CoreStorageKey {
        CoreStorageKey::ProfileReferencesReceived {
            root_public_key: self.root_public_key,
            genesis_hash: self.genesis_hash,
            product_id: product_id.to_string(),
        }
    }
}

/// The user's own disclosed reference and the product that disclosed it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct Disclosure {
    pub(crate) product_id: String,
    pub(crate) reference: String,
}

/// What one contact's host last sent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct ReceivedReference {
    pub(crate) peer_identity: [u8; 32],
    /// The product on the contact's side that disclosed it.
    pub(crate) discloser_product_id: String,
    /// Sender timestamp of the frame this reflects; only a later frame
    /// replaces it.
    pub(crate) timestamp: u64,
    /// `None` once withdrawn. The withdrawal is kept, so an older disclosure
    /// opened after it cannot bring the reference back.
    pub(crate) reference: Option<String>,
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
    owner: ProfileOwner,
) -> Result<Option<Disclosure>, String> {
    let Some(raw) = storage
        .read_core_storage(owner.disclosure_key())
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
    owner: ProfileOwner,
    disclosure: &Disclosure,
) -> Result<(), String> {
    storage
        .write_core_storage(owner.disclosure_key(), disclosure.encode())
        .await
        .map_err(storage_error)
}

pub(crate) async fn clear_disclosure(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<(), String> {
    storage
        .clear_core_storage(owner.disclosure_key())
        .await
        .map_err(storage_error)
}

async fn read_received(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
) -> Result<Vec<ReceivedReference>, String> {
    let Some(raw) = storage
        .read_core_storage(owner.received_key(product_id))
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

/// What a contact's host last sent this product's user, withdrawals included.
pub(crate) async fn received_reference(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
    peer_identity: &[u8; 32],
) -> Result<Option<ReceivedReference>, String> {
    Ok(read_received(storage, owner, product_id)
        .await?
        .into_iter()
        .find(|entry| &entry.peer_identity == peer_identity))
}

/// Record a frame a contact's host sent, if it is newer than the one held:
/// a reference replaces the old one, and `None` withdraws it. A frame that is
/// not strictly newer is a replay or was overtaken, and changes nothing.
/// `true` when the frame was kept.
pub(crate) async fn record_received_reference(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
    peer_identity: [u8; 32],
    discloser_product_id: String,
    timestamp: u64,
    reference: Option<String>,
) -> Result<bool, String> {
    let mut entries = read_received(storage, owner, product_id).await?;
    let received = ReceivedReference {
        peer_identity,
        discloser_product_id,
        timestamp,
        reference,
    };
    match entries
        .iter()
        .position(|entry| entry.peer_identity == peer_identity)
    {
        Some(index) if entries[index].timestamp >= timestamp => return Ok(false),
        Some(index) => entries[index] = received,
        None if entries.len() >= MAX_RECEIVED_REFERENCES => {
            return Err("too many contact profile references".to_string());
        }
        None => entries.push(received),
    }
    storage
        .write_core_storage(
            owner.received_key(product_id),
            StoredReferences::V1(entries).encode(),
        )
        .await
        .map_err(storage_error)?;
    Ok(true)
}
