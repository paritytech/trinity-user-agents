//! Contact avatars the host draws over a chat product.
//!
//! A product says where it draws each contact's avatar, by peer identity. The
//! core fills in the reference each contact shared and hands the host only the
//! avatars it can draw. The product gets the same answer whoever shared, and
//! nothing about a slot is logged, so it cannot learn who shared a profile.
//!
//! The placement is kept per product connection, so a contact who shares or
//! withdraws later appears or disappears without the product sending it again.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tracing::debug;
use truapi::v01;
use truapi_platform::{PlacedAvatar, PlacedAvatars, Platform, ProductContext, ProfilePlatform};

use super::{ProfileOwner, read_received};
use crate::runtime::is_screened_profile_reference;
use crate::subscription::Spawner;

/// Most avatars one placement may hold: a screenful of list rows and a header.
const MAX_SLOTS: usize = 64;
/// Longest surface side, in surface units.
const MAX_SURFACE_SIDE: u32 = 16384;
/// Longest avatar side, in surface units.
const MAX_AVATAR_SIDE: u32 = 1024;

/// Why a placement is malformed, if it is. Only input the product controls is
/// judged here, never what any contact shared.
pub(crate) fn validate(request: &v01::HostProfilePlaceContactAvatarsRequest) -> Result<(), String> {
    let surface = 1..=MAX_SURFACE_SIDE;
    if !surface.contains(&request.surface_width) || !surface.contains(&request.surface_height) {
        return Err(format!("surface sides must be 1 to {MAX_SURFACE_SIDE}"));
    }
    if request.slots.len() > MAX_SLOTS {
        return Err(format!("at most {MAX_SLOTS} contact avatars may be placed"));
    }
    let mut seen = HashSet::with_capacity(request.slots.len());
    for slot in &request.slots {
        let rect = slot.rect;
        if rect.width != rect.height || !(1..=MAX_AVATAR_SIDE).contains(&rect.width) {
            return Err(format!(
                "avatar {} must be square and 1 to {MAX_AVATAR_SIDE} a side",
                slot.slot
            ));
        }
        if !seen.insert(slot.slot) {
            return Err(format!("avatar slot {} is placed twice", slot.slot));
        }
    }
    Ok(())
}

/// One product connection's placement.
pub(crate) struct ContactAvatarPlacement {
    platform: Arc<dyn ProfilePlatform>,
    storage: Arc<dyn Platform>,
    product: ProductContext,
    /// Held across each draw, so the host sees the connection's placements in
    /// the order they were made.
    state: futures::lock::Mutex<PlacementState>,
}

#[derive(Default)]
struct PlacementState {
    /// The last non-empty placement and the wallet it was drawn for.
    placed: Option<(ProfileOwner, v01::HostProfilePlaceContactAvatarsRequest)>,
    /// The connection is gone; nothing is drawn for it again.
    closed: bool,
}

impl ContactAvatarPlacement {
    pub(crate) fn new(
        platform: Arc<dyn ProfilePlatform>,
        storage: Arc<dyn Platform>,
        product: ProductContext,
    ) -> Self {
        Self {
            platform,
            storage,
            product,
            state: futures::lock::Mutex::new(PlacementState::default()),
        }
    }

    /// Replace the placement and draw it for `owner`. Only the host's own
    /// `Unsupported` and an unreadable store fail; whatever was drawn, the
    /// answer is `Ok`.
    pub(crate) async fn place(
        &self,
        owner: ProfileOwner,
        request: v01::HostProfilePlaceContactAvatarsRequest,
    ) -> Result<(), v01::HostProfilePlaceContactAvatarsError> {
        let mut state = self.state.lock().await;
        if state.closed {
            return Ok(());
        }
        state.placed = None;
        self.draw(owner, &request).await?;
        if !request.slots.is_empty() {
            state.placed = Some((owner, request));
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
        let Some((_, request)) = state.placed.take() else {
            return;
        };
        let cleared = PlacedAvatars {
            surface_width: request.surface_width,
            surface_height: request.surface_height,
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

    /// Draw the placement again after what `owner`'s contacts shared changed.
    async fn redraw(&self, owner: ProfileOwner) {
        let state = self.state.lock().await;
        let Some((placed_for, request)) = state.placed.as_ref() else {
            return;
        };
        if *placed_for != owner {
            return;
        }
        if let Err(error) = self.draw(owner, request).await {
            debug!(?error, "contact avatars were not redrawn");
        }
    }

    async fn draw(
        &self,
        owner: ProfileOwner,
        request: &v01::HostProfilePlaceContactAvatarsRequest,
    ) -> Result<(), v01::HostProfilePlaceContactAvatarsError> {
        let avatars = self
            .drawable(owner, &request.slots)
            .await
            .map_err(|reason| v01::HostProfilePlaceContactAvatarsError::Unknown { reason })?;
        let placed = PlacedAvatars {
            surface_width: request.surface_width,
            surface_height: request.surface_height,
            avatars,
        };
        match self
            .platform
            .place_contact_avatars(&self.product, placed)
            .await
        {
            Ok(()) => Ok(()),
            Err(v01::HostProfilePlaceContactAvatarsError::Unsupported) => {
                Err(v01::HostProfilePlaceContactAvatarsError::Unsupported)
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
    /// user, each with that contact's reference.
    async fn drawable(
        &self,
        owner: ProfileOwner,
        slots: &[v01::ContactAvatarSlot],
    ) -> Result<Vec<PlacedAvatar>, String> {
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let shared: HashMap<[u8; 32], String> =
            read_received(self.storage.as_ref(), owner, &self.product.product_id)
                .await?
                .into_iter()
                .filter_map(|received| {
                    let reference = received.reference?;
                    is_screened_profile_reference(&reference)
                        .then_some((received.peer_identity, reference))
                })
                .collect();
        Ok(slots
            .iter()
            .filter_map(|slot| {
                shared
                    .get(&slot.peer_identity)
                    .map(|reference| PlacedAvatar {
                        slot: slot.slot,
                        rect: slot.rect,
                        clip: slot.clip,
                        reference: reference.clone(),
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
}
