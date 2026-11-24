//! Username helpers on [`SessionInfo`], kept out of its public inherent
//! interface so they stay internal to the runtime.

/// Username helpers for session state.
pub trait SessionUsernames {
	/// Whether the session already carries a usable username.
	fn has_username(&self) -> bool;

	/// Whether `other` is this session, whatever usernames either carries.
	///
	/// Usernames are looked up on the chains and change while a session stays
	/// selected, so a session whose usernames change is not replaced: its
	/// grants, allowance keys and SSO channel stay.
	fn is_same_session(&self, other: &Self) -> bool;
}
