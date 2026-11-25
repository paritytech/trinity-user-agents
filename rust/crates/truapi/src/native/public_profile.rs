//! Native callbacks for finite public profile resolution.

use crate::{PublicProfile, PublicProfileError};

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
