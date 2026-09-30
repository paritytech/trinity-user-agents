//! Profile disclosure state: the reference the user disclosed to their chat
//! contacts, and the references their contacts disclosed to them.
//!
//! Both are bearer capabilities. They live in core storage, never in product
//! storage, and never cross back to a product: `present_contact` and placed
//! contact avatars name a contact and the host substitutes the reference. Both
//! belong to one wallet on one Chat network, like the roster they travel over.

pub(crate) mod avatars;

use crate::platform::{CoreStorage, CoreStorageKey};
use parity_scale_codec::{Decode, DecodeAll, Encode};

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
    pub(crate) fn disclosure_key(&self) -> CoreStorageKey {
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

    fn personal_received_key(&self) -> CoreStorageKey {
        CoreStorageKey::ProfilePersonalReferencesReceived {
            root_public_key: self.root_public_key,
            genesis_hash: self.genesis_hash,
        }
    }
}

/// The user's own disclosed reference and the product that disclosed it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct Disclosure {
    pub(crate) product_id: String,
    pub(crate) reference: String,
    /// Which `profile.disclose` call this is. Every call takes a larger
    /// revision, so disclosing the same reference again (the record behind
    /// it changed) starts a new round to every contact. `0` for a disclosure
    /// stored before revisions existed.
    pub(crate) revision: u64,
    /// Legacy sharing to every ready peer of every authorized Chat app.
    pub(crate) all_chat_apps: bool,
    /// Selected app-scoped audiences, independent of personal grants.
    pub(crate) app_products: Vec<String>,
    /// Exact authenticated peer identity accounts selected through Contacts.
    pub(crate) contacts: Vec<[u8; 32]>,
}

/// A disclosure written before selected audiences existed.
#[derive(Decode)]
struct AllChatDisclosure {
    product_id: String,
    reference: String,
    revision: u64,
}

/// Independent grants for the receiving app or the receiving wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub(crate) enum ProfileScope {
    #[codec(index = 0)]
    App,
    #[codec(index = 1)]
    Personal,
}

impl Disclosure {
    pub(crate) fn grants(&self, scope: ProfileScope, product: &str, peer: &[u8; 32]) -> bool {
        match scope {
            ProfileScope::App => {
                self.all_chat_apps || self.app_products.iter().any(|id| id == product)
            }
            ProfileScope::Personal => self.contacts.contains(peer),
        }
    }
}

/// A disclosure as stored before revisions: product and reference only.
#[derive(Decode)]
struct UnrevisedDisclosure {
    product_id: String,
    reference: String,
}

/// What one contact's host last sent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct ReceivedReference {
    pub(crate) peer_identity: [u8; 32],
    /// The product on the contact's side that disclosed it.
    pub(crate) discloser_product_id: String,
    /// Frame freshness timestamp. Personal grants order by durable revision
    /// and advance this value even when another actor's relay clock is older.
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

#[derive(Encode, Decode)]
struct PersonalReference {
    received: ReceivedReference,
    revision: u64,
}

const DISCLOSURE_MARKER: [u8; 4] = [0xff, b'P', b'D', 2];

/// A contact roster is bounded; so is what the host keeps for it.
const MAX_RECEIVED_REFERENCES: usize = 4096;

fn storage_error(error: impl core::fmt::Debug) -> String {
    format!("profile storage failed: {error:?}")
}

pub(crate) async fn read_disclosure(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<Option<Disclosure>, String> {
    Ok(read_disclosure_state(storage, owner).await?.1)
}

/// The sequence survives retraction so every Chat app orders personal grants alike.
pub(crate) async fn read_disclosure_state(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<(u64, Option<Disclosure>), String> {
    let Some(raw) = storage
        .read_core_storage(owner.disclosure_key())
        .await
        .map_err(storage_error)?
    else {
        return Ok((0, None));
    };
    let bytes = raw.as_slice();
    if let Some(current) = bytes.strip_prefix(&DISCLOSURE_MARKER) {
        let state = <(u64, Option<Disclosure>)>::decode_all(&mut &current[..])
            .map_err(|error| format!("stored profile disclosure is unreadable: {error}"))?;
        if state
            .1
            .as_ref()
            .is_some_and(|disclosure| disclosure.revision != state.0)
        {
            return Err("stored profile revision is inconsistent".into());
        }
        return Ok(state);
    }
    if let Ok(old) = AllChatDisclosure::decode_all(&mut &bytes[..]) {
        return Ok((
            old.revision,
            Some(Disclosure {
                product_id: old.product_id,
                reference: old.reference,
                revision: old.revision,
                all_chat_apps: true,
                app_products: Vec::new(),
                contacts: Vec::new(),
            }),
        ));
    }
    UnrevisedDisclosure::decode_all(&mut &bytes[..])
        .map(|old| {
            (
                0,
                Some(Disclosure {
                    product_id: old.product_id,
                    reference: old.reference,
                    revision: 0,
                    all_chat_apps: true,
                    app_products: Vec::new(),
                    contacts: Vec::new(),
                }),
            )
        })
        .map_err(|error| format!("stored profile disclosure is unreadable: {error}"))
}

pub(crate) async fn write_disclosure(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    disclosure: &Disclosure,
) -> Result<(), String> {
    let previous = read_disclosure_state(storage, owner).await?.0;
    let revision = disclosure.revision.max(
        previous
            .checked_add(1)
            .ok_or("profile revision exhausted")?,
    );
    let fields = (
        &disclosure.product_id,
        &disclosure.reference,
        revision,
        disclosure.all_chat_apps,
        &disclosure.app_products,
        &disclosure.contacts,
    );
    let bytes = (DISCLOSURE_MARKER, revision, Some(fields)).encode();
    storage
        .write_core_storage(owner.disclosure_key(), bytes)
        .await
        .map_err(storage_error)
}

pub(crate) async fn clear_disclosure(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<(), String> {
    let revision = read_disclosure_state(storage, owner)
        .await?
        .0
        .checked_add(1)
        .ok_or("profile revision exhausted")?;
    storage
        .write_core_storage(
            owner.disclosure_key(),
            (DISCLOSURE_MARKER, revision, None::<Disclosure>).encode(),
        )
        .await
        .map_err(storage_error)
}

async fn read_received_slot(
    storage: &(impl CoreStorage + ?Sized),
    key: CoreStorageKey,
) -> Result<Vec<ReceivedReference>, String> {
    let Some(raw) = storage
        .read_core_storage(key)
        .await
        .map_err(storage_error)?
    else {
        return Ok(Vec::new());
    };
    match StoredReferences::decode_all(&mut raw.as_slice()) {
        Ok(StoredReferences::V1(entries)) if entries.len() <= MAX_RECEIVED_REFERENCES => {
            Ok(entries)
        }
        Ok(_) => Err("too many contact profile references".to_string()),
        Err(error) => Err(format!("stored profile references are unreadable: {error}")),
    }
}

async fn read_personal_received(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<Vec<PersonalReference>, String> {
    let Some(raw) = storage
        .read_core_storage(owner.personal_received_key())
        .await
        .map_err(storage_error)?
    else {
        return Ok(Vec::new());
    };
    let (version, entries) = <(u8, Vec<PersonalReference>)>::decode_all(&mut raw.as_slice())
        .map_err(|error| format!("stored personal profile references are unreadable: {error}"))?;
    if version != 1 || entries.len() > MAX_RECEIVED_REFERENCES {
        return Err("invalid personal profile references".into());
    }
    Ok(entries)
}

async fn read_received(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
) -> Result<Vec<ReceivedReference>, String> {
    use std::collections::{BTreeMap, btree_map::Entry};
    let mut entries = read_received_slot(storage, owner.received_key(product_id))
        .await?
        .into_iter()
        .map(|entry| (entry.peer_identity, entry))
        .collect::<BTreeMap<_, _>>();
    for PersonalReference { received, .. } in read_personal_received(storage, owner).await? {
        match entries.entry(received.peer_identity) {
            Entry::Occupied(mut held)
                if held.get().reference.is_none() && received.reference.is_some() =>
            {
                held.insert(received);
            }
            Entry::Vacant(slot) => {
                slot.insert(received);
            }
            _ => {}
        }
    }
    Ok(entries.into_values().collect())
}

/// App-only lookup for APIs whose result may reveal whether an app grant exists.
pub(crate) async fn received_app_reference(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
    peer_identity: &[u8; 32],
) -> Result<Option<ReceivedReference>, String> {
    Ok(read_received_slot(storage, owner.received_key(product_id))
        .await?
        .into_iter()
        .find(|entry| &entry.peer_identity == peer_identity))
}

/// Effective grant: a live app reference takes precedence over a personal grant.
pub(crate) async fn received_reference(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    product_id: &str,
    peer_identity: &[u8; 32],
) -> Result<Option<ReceivedReference>, String> {
    let app = received_app_reference(storage, owner, product_id, peer_identity).await?;
    if app.as_ref().is_some_and(|entry| entry.reference.is_some()) {
        return Ok(app);
    }
    let personal = read_personal_received(storage, owner)
        .await?
        .into_iter()
        .find(|entry| &entry.received.peer_identity == peer_identity)
        .map(|entry| entry.received);
    Ok(match personal {
        Some(personal) if personal.reference.is_some() || app.is_none() => Some(personal),
        _ => app,
    })
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
    let key = owner.received_key(product_id);
    let mut entries = read_received_slot(storage, key.clone()).await?;
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
        .write_core_storage(key, StoredReferences::V1(entries).encode())
        .await
        .map_err(storage_error)?;
    Ok(true)
}

/// Record a personal frame by the sender's durable revision, not its relay time.
/// Callers serialize updates across products with the host's profile state gate.
pub(crate) async fn record_personal_received_reference(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    peer_identity: [u8; 32],
    discloser_product_id: String,
    timestamp: u64,
    revision: u64,
    reference: Option<String>,
) -> Result<bool, String> {
    if revision == 0 {
        return Err("invalid personal profile revision".into());
    }
    let mut entries = read_personal_received(storage, owner).await?;
    let mut entry = PersonalReference {
        received: ReceivedReference {
            peer_identity,
            discloser_product_id,
            timestamp,
            reference,
        },
        revision,
    };
    match entries
        .iter()
        .position(|held| held.received.peer_identity == peer_identity)
    {
        Some(index) if entries[index].revision >= revision => return Ok(false),
        Some(index) => {
            // Hosts invalidate resolved profile caches using shared_at.
            // Cross-app ordering is by revision, but that must also advance
            // the render token when the newer actor's clock is behind.
            entry.received.timestamp = timestamp.max(
                entries[index]
                    .received
                    .timestamp
                    .checked_add(1)
                    .ok_or("personal profile freshness exhausted")?,
            );
            entries[index] = entry;
        }
        None if entries.len() >= MAX_RECEIVED_REFERENCES => {
            return Err("too many personal profile references".into());
        }
        None => entries.push(entry),
    }
    storage
        .write_core_storage(owner.personal_received_key(), (1u8, entries).encode())
        .await
        .map_err(storage_error)?;
    Ok(true)
}
