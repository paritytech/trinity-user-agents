//! The chats every host places itself.

use crate::platform::{PrivilegedProduct, ProductId, ProductIdError};

/// A chat with a privileged product that every host lists from the first
/// run, with no add, install or discover step, and that nobody can leave.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct HostPlacedChat {
	/// Product the chat is with.
	pub product_id: ProductId,
	/// Room the product registers for the chat.
	pub room_id: String,
	/// Name shown until the product's manifest supplies its own.
	pub name: String,
}

/// The chats every host places on the network with `network_suffix`.
///
/// Fails when `network_suffix` is not a dotNS TLD.
#[uniffi::export]
pub fn host_placed_chats(network_suffix: String) -> Result<Vec<HostPlacedChat>, ProductIdError> {
	Ok(vec![HostPlacedChat {
		product_id: PrivilegedProduct::Game.product_id(&network_suffix)?,
		room_id: "jollity".into(),
		name: "Jollity".into(),
	}])
}

#[cfg(test)]
mod tests;
