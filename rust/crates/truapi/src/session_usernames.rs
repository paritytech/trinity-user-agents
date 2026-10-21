//! Username helpers on [`SessionInfo`], kept out of its public inherent
//! interface so they stay internal to the runtime.

/// Username presence helper for session state.
pub trait SessionUsernames {
	/// Whether the session already carries a usable username.
	fn has_username(&self) -> bool;
}
