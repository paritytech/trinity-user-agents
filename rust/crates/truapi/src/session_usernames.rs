//! Username helpers on [`SessionInfo`], kept out of its public inherent
//! interface so they stay internal to the runtime.

/// Username presence and merge helpers for session state.
pub trait SessionUsernames {
	/// Whether the session already carries a usable username.
	fn has_username(&self) -> bool;

	/// Apply resolved username fields without replacing populated values with
	/// empty strings.
	fn apply_usernames(&mut self, lite_username: Option<String>, full_username: Option<String>);
}
