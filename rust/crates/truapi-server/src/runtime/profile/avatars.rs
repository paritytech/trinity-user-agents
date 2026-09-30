//! Avatars the host draws over a chat product: its contacts' and the signed-in
//! user's own.
//!
//! A product says where it draws each contact's avatar, by identity or handle, and
//! optionally where it draws the user's own. The core fills in the reference
//! each contact shared, and the user's own disclosure, and hands the host only
//! the avatars it can draw. The product gets the same answer whoever shared, and
//! nothing about a slot is logged, so it cannot learn who shared a profile.
//!
//! The placement is kept per product connection, so a contact who shares or
//! withdraws later, or the user disclosing or retracting their own, appears or
//! disappears without the product sending it again.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

use crate::platform::{PlacedAvatar, PlacedAvatars, Platform, ProductContext, ProfilePlatform};
use tracing::debug;
use truapi::latest::{
    ContactAvatarSlot, HostProfilePlaceContactAvatarsError, HostProfilePlaceContactAvatarsRequest,
    ProfileContact,
};

use super::{ProfileOwner, read_disclosure, read_received};
use crate::runtime::{
    ProductAuthority, RuntimeServices, contacts::ContactHandles, is_screened_profile_reference,
    resolve_contact_accounts,
};
use crate::subscription::Spawner;

/// Most avatars one placement may hold: a screenful of list rows and a header.
const MAX_SLOTS: usize = 64;
/// Longest surface side, in surface units.
const MAX_SURFACE_SIDE: u32 = 16384;
/// Longest avatar side, in surface units.
const MAX_AVATAR_SIDE: u32 = 1024;

/// Why a placement is malformed, if it is. Only input the product controls is
/// judged here, never what any contact shared.
pub(crate) fn validate(request: &HostProfilePlaceContactAvatarsRequest) -> Result<(), String> {
    let surface = 1..=MAX_SURFACE_SIDE;
    if !surface.contains(&request.surface_width) || !surface.contains(&request.surface_height) {
        return Err(format!("surface sides must be 1 to {MAX_SURFACE_SIDE}"));
    }
    if request.slots.len() > MAX_SLOTS {
        return Err(format!("at most {MAX_SLOTS} contact avatars may be placed"));
    }
    let mut seen = HashSet::with_capacity(request.slots.len() + usize::from(request.own.is_some()));
    let own = request.own.iter().map(|own| (own.slot, own.rect));
    for (slot, rect) in own.chain(request.slots.iter().map(|slot| (slot.slot, slot.rect))) {
        if rect.width != rect.height || !(1..=MAX_AVATAR_SIDE).contains(&rect.width) {
            return Err(format!(
                "avatar {slot} must be square and 1 to {MAX_AVATAR_SIDE} a side"
            ));
        }
        if !seen.insert(slot) {
            return Err(format!("avatar slot {slot} is placed twice"));
        }
    }
    Ok(())
}

/// One product connection's placement.
pub(crate) struct ContactAvatarPlacement {
    platform: Arc<dyn ProfilePlatform>,
    storage: Arc<dyn Platform>,
    product: ProductContext,
    services: Weak<RuntimeServices>,
    /// Held across each draw, so the host sees the connection's placements in
    /// the order they were made.
    state: futures::lock::Mutex<PlacementState>,
}

struct RememberedPlacement {
    owner: ProfileOwner,
    request: HostProfilePlaceContactAvatarsRequest,
    authority: Option<Weak<dyn ProductAuthority>>,
}

#[derive(Default)]
struct PlacementState {
    /// The last non-empty placement and the wallet it was drawn for.
    placed: Option<RememberedPlacement>,
    /// The connection is gone; nothing is drawn for it again.
    closed: bool,
}

impl ContactAvatarPlacement {
    pub(crate) fn new(
        platform: Arc<dyn ProfilePlatform>,
        storage: Arc<dyn Platform>,
        product: ProductContext,
        services: Weak<RuntimeServices>,
    ) -> Self {
        Self {
            platform,
            storage,
            product,
            services,
            state: futures::lock::Mutex::new(PlacementState::default()),
        }
    }

    /// Replace the placement and draw it for `owner`. Only the host's own
    /// `Unsupported` and an unreadable store fail; whatever was drawn, the
    /// answer is `Ok`.
    pub(crate) async fn place(
        &self,
        owner: ProfileOwner,
        request: HostProfilePlaceContactAvatarsRequest,
        authority: Option<Weak<dyn ProductAuthority>>,
    ) -> Result<(), HostProfilePlaceContactAvatarsError> {
        let mut state = self.state.lock().await;
        if state.closed {
            return Ok(());
        }
        state.placed = None;
        self.draw(owner, &request, authority.as_ref()).await?;
        if request.own.is_some() || !request.slots.is_empty() {
            state.placed = Some(RememberedPlacement {
                owner,
                request,
                authority,
            });
        }
        Ok(())
    }

    /// Forget the placement and clear what the host drew for it.
    pub(crate) async fn clear(&self) {
        let mut state = self.state.lock().await;
        self.clear_drawn(&mut state).await;
    }

    /// Clear what the host drew and draw nothing for this connection again.
    async fn close(&self) {
        let mut state = self.state.lock().await;
        state.closed = true;
        self.clear_drawn(&mut state).await;
    }

    async fn clear_drawn(&self, state: &mut PlacementState) {
        let Some(RememberedPlacement { request, .. }) = state.placed.take() else {
            return;
        };
        let (surface_width, surface_height) = (request.surface_width, request.surface_height);
        let cleared = PlacedAvatars {
            surface_width,
            surface_height,
            avatars: Vec::new(),
        };
        if let Err(error) = self
            .platform
            .place_contact_avatars(&self.product, cleared)
            .await
        {
            debug!(?error, "host could not clear contact avatars");
        }
    }

    /// Draw the placement again after what `owner`'s contacts shared, or what
    /// `owner` disclosed, changed.
    async fn redraw(&self, owner: ProfileOwner) {
        let state = self.state.lock().await;
        let Some(RememberedPlacement {
            owner: placed_for,
            request,
            authority,
        }) = state.placed.as_ref()
        else {
            return;
        };
        if *placed_for != owner {
            return;
        }
        if let Err(error) = self.draw(owner, request, authority.as_ref()).await {
            debug!(?error, "contact avatars were not redrawn");
        }
    }

    async fn contacts_changed(&self) {
        let state = self.state.lock().await;
        let Some(RememberedPlacement {
            owner,
            request,
            authority,
        }) = state.placed.as_ref()
        else {
            return;
        };
        if request
            .slots
            .iter()
            .any(|slot| matches!(slot.contact, ProfileContact::Handle { .. }))
        {
            let _ = self
                .platform
                .place_contact_avatars(
                    &self.product,
                    PlacedAvatars {
                        surface_width: request.surface_width,
                        surface_height: request.surface_height,
                        avatars: Vec::new(),
                    },
                )
                .await;
            if let Err(error) = self.draw(*owner, request, authority.as_ref()).await {
                debug!(?error, "contact avatars were not redrawn");
            }
        }
    }

    async fn draw(
        &self,
        owner: ProfileOwner,
        request: &HostProfilePlaceContactAvatarsRequest,
        authority: Option<&Weak<dyn ProductAuthority>>,
    ) -> Result<(), HostProfilePlaceContactAvatarsError> {
        let unknown = |reason| HostProfilePlaceContactAvatarsError::Unknown { reason };
        let mut own_avatar = None;
        if let Some(own) = request.own {
            let disclosure = read_disclosure(self.storage.as_ref(), owner)
                .await
                .map_err(unknown)?;
            if let Some(disclosure) =
                disclosure.filter(|disclosure| is_screened_profile_reference(&disclosure.reference))
            {
                own_avatar = Some(PlacedAvatar {
                    slot: own.slot,
                    rect: own.rect,
                    clip: own.clip,
                    reference: disclosure.reference,
                    // Every disclosure takes a newer revision, so a host that
                    // caches by `shared_at` refetches the user's new profile.
                    shared_at: disclosure.revision,
                });
            }
        }
        let mut avatars = self
            .drawable(owner, &request.slots, authority)
            .await
            .map_err(unknown)?;
        if let Some(own_avatar) = own_avatar {
            avatars.push(own_avatar);
        }
        let (surface_width, surface_height) = (request.surface_width, request.surface_height);
        let placed = PlacedAvatars {
            surface_width,
            surface_height,
            avatars,
        };
        match self
            .platform
            .place_contact_avatars(&self.product, placed)
            .await
        {
            Ok(()) => Ok(()),
            Err(HostProfilePlaceContactAvatarsError::Unsupported) => {
                Err(HostProfilePlaceContactAvatarsError::Unsupported)
            }
            // Any other host failure could depend on which avatars it was
            // given, so the product is not told of it.
            Err(error) => {
                debug!(?error, "host could not draw contact avatars");
                Ok(())
            }
        }
    }

    /// The slots whose contact currently shares a profile with this product's
    /// user, each with that contact's reference and when it was shared.
    async fn drawable(
        &self,
        owner: ProfileOwner,
        slots: &[ContactAvatarSlot],
        authority: Option<&Weak<dyn ProductAuthority>>,
    ) -> Result<Vec<PlacedAvatar>, String> {
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let shared: HashMap<[u8; 32], (String, u64)> =
            read_received(self.storage.as_ref(), owner, &self.product.product_id)
                .await?
                .into_iter()
                .filter_map(|received| {
                    let reference = received.reference?;
                    is_screened_profile_reference(&reference)
                        .then_some((received.peer_identity, (reference, received.timestamp)))
                })
                .collect();
        let requested: Vec<[u8; 32]> = slots
            .iter()
            .filter_map(|slot| match slot.contact {
                ProfileContact::Handle { handle } => Some(handle.bytes),
                ProfileContact::Peer { .. } => None,
            })
            .collect();
        let services = self.services.upgrade();
        let mut generation = None;
        let resolved = if requested.is_empty() {
            Vec::new()
        } else if let (Some(services), Some(authority)) =
            (services.as_ref(), authority.and_then(Weak::upgrade))
        {
            generation = Some(services.contact_handles.generation());
            if let (Some(platform), Some(session)) = (
                services.contacts_platform(),
                authority
                    .current_session()
                    .filter(|session| session.public_key == owner.root_public_key),
            ) {
                if let Ok(handle_key) = authority.contacts_handle_key(&session) {
                    let resolved = resolve_contact_accounts(
                        services,
                        platform.as_ref(),
                        &ContactHandles::from_handle_key(handle_key),
                        &requested,
                    )
                    .await
                    .unwrap_or_default();
                    if authority.current_session().as_ref() == Some(&session) {
                        resolved
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let handles_current = services
            .as_ref()
            .is_some_and(|services| generation == Some(services.contact_handles.generation()));
        Ok(slots
            .iter()
            .filter_map(|slot| {
                let identity = match slot.contact {
                    ProfileContact::Peer { peer_identity } => peer_identity,
                    ProfileContact::Handle { handle } if handles_current => {
                        resolved
                            .iter()
                            .find(|(requested, _)| *requested == handle.bytes)?
                            .1?
                    }
                    ProfileContact::Handle { .. } => return None,
                };
                shared
                    .get(&identity)
                    .map(|(reference, shared_at)| PlacedAvatar {
                        slot: slot.slot,
                        rect: slot.rect,
                        clip: slot.clip,
                        reference: reference.clone(),
                        shared_at: *shared_at,
                    })
            })
            .collect())
    }
}

/// Every live product connection's placement, by product runtime.
#[derive(Default)]
pub(crate) struct ContactAvatarPlacements {
    by_runtime: Mutex<HashMap<u64, Arc<ContactAvatarPlacement>>>,
}

impl ContactAvatarPlacements {
    /// The placement of product runtime `runtime`, made on first use.
    pub(crate) fn for_runtime(
        &self,
        runtime: u64,
        make: impl FnOnce() -> ContactAvatarPlacement,
    ) -> Arc<ContactAvatarPlacement> {
        self.by_runtime
            .lock()
            .expect("contact avatar placements mutex poisoned")
            .entry(runtime)
            .or_insert_with(|| Arc::new(make()))
            .clone()
    }

    /// Clear what the host drew for product runtime `runtime` and forget it.
    pub(crate) fn release(&self, runtime: u64, spawner: &Spawner) {
        let Some(placement) = self
            .by_runtime
            .lock()
            .expect("contact avatar placements mutex poisoned")
            .remove(&runtime)
        else {
            return;
        };
        spawner(Box::pin(async move { placement.close().await }));
    }

    /// Redraw every placement `product_id` holds for `owner`, after what that
    /// product's contacts shared changed.
    pub(crate) fn redraw(&self, owner: ProfileOwner, product_id: &str, spawner: &Spawner) {
        let placements = self
            .by_runtime
            .lock()
            .expect("contact avatar placements mutex poisoned")
            .values()
            .filter(|placement| placement.product.product_id == product_id)
            .cloned()
            .collect::<Vec<_>>();
        if placements.is_empty() {
            return;
        }
        spawner(Box::pin(async move {
            for placement in placements {
                placement.redraw(owner).await;
            }
        }));
    }

    /// Redraw every placement for `owner` after their own disclosed profile
    /// changed. Any product may draw the user's own avatar, so every
    /// placement is redrawn, not only the discloser's.
    pub(crate) fn redraw_owner(&self, owner: ProfileOwner, spawner: &Spawner) {
        let placements = self
            .by_runtime
            .lock()
            .expect("contact avatar placements mutex poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if placements.is_empty() {
            return;
        }
        spawner(Box::pin(async move {
            for placement in placements {
                placement.redraw(owner).await;
            }
        }));
    }

    /// Re-resolve handle placements after the host removes or blocks contacts.
    pub fn contacts_changed(&self, spawner: &Spawner) {
        let placements = self
            .by_runtime
            .lock()
            .expect("contact avatar placements mutex poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if placements.is_empty() {
            return;
        }
        spawner(Box::pin(async move {
            for placement in placements {
                placement.contacts_changed().await;
            }
        }));
    }
}
