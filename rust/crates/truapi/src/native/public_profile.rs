//! Cached and refreshed public profile metadata delivered by [`observe_public_profile`].

use std::sync::Arc;

use futures::StreamExt;

use super::renderer::NativeSubscription;
use crate::{PublicProfile, PublicProfileError, Spawner, Subscription};

/// Observer implemented by a native host to receive one profile resolution.
#[uniffi::export(foreign)]
pub trait NativePublicProfileObserver: Send + Sync {
	/// Receives cached metadata or a changed resolved profile, at most twice.
	fn on_profile(&self, profile: PublicProfile);

	/// Reports that profile resolution completed without further values.
	fn on_complete(&self);

	/// Reports a terminal failure, preserving any profile already delivered.
	fn on_error(&self, error: PublicProfileError);
}

/// Delivers the profile stream until it completes, fails, or is cancelled.
///
/// The returned subscription aborts the resolution future when cancelled or dropped.
pub fn observe_public_profile(
	mut stream: Subscription<PublicProfile, PublicProfileError>,
	observer: Arc<dyn NativePublicProfileObserver>,
	spawner: &Spawner,
) -> Arc<NativeSubscription> {
	spawner.spawn_observation(async move {
		while let Some(item) = stream.next().await {
			match item {
				Ok(profile) => observer.on_profile(profile),
				Err(error) => return observer.on_error(error),
			}
		}
		observer.on_complete();
	})
}

#[cfg(test)]
mod tests;
