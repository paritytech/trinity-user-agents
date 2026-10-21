//! The person chat's host services: the HOP nodes of the network, the
//! devices paired with the wallet (none on the CLI), and the chat's contacts
//! (which nothing else on the CLI shows).

use async_trait::async_trait;
use truapi::{
	latest::GenericError,
	platform::{
		HostChatContact, HostHistoryExportChunk, HostPairedDevice, PersonChatPlatform,
		ProductContext,
	},
};

use crate::network::{HOP_NODES_ENV, NetworkConfig};

/// Serves the preset's HOP nodes, or those of [`HOP_NODES_ENV`].
pub struct CliPersonChatHost {
	nodes: Vec<String>,
}

impl CliPersonChatHost {
	/// The HOP nodes of `network`, replaced by [`HOP_NODES_ENV`] when it is set.
	pub fn for_network(network: &NetworkConfig) -> Self {
		Self::from_list(network, std::env::var(HOP_NODES_ENV).ok())
	}

	fn from_list(network: &NetworkConfig, list: Option<String>) -> Self {
		let nodes = match list {
			Some(list) => list
				.split(',')
				.map(str::trim)
				.filter(|node| !node.is_empty())
				.map(str::to_string)
				.collect(),
			None => network.hop_nodes.iter().map(|node| node.to_string()).collect(),
		};
		Self { nodes }
	}
}

#[async_trait]
impl PersonChatPlatform for CliPersonChatHost {
	async fn hop_nodes(&self) -> Result<Vec<String>, GenericError> {
		Ok(self.nodes.clone())
	}

	// The CLI hosts keep no list of devices paired with them.
	async fn paired_devices(&self) -> Result<Vec<HostPairedDevice>, GenericError> {
		Ok(Vec::new())
	}

	// Nothing outside the chat shows contacts on the CLI.
	async fn set_contacts(
		&self,
		_product: &ProductContext,
		contacts: Vec<HostChatContact>,
	) -> Result<(), GenericError> {
		tracing::debug!(contacts = contacts.len(), "chat contacts published");
		Ok(())
	}

	// The CLI hosts have no native chat to move over.
	async fn history_export(
		&self,
		_product: &ProductContext,
		_offset: u64,
		_max_len: u32,
	) -> Result<HostHistoryExportChunk, GenericError> {
		Ok(HostHistoryExportChunk { total: 0, data: Vec::new() })
	}

	async fn history_imported(&self, _product: &ProductContext) -> Result<(), GenericError> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::network::Network;

	#[test]
	fn the_preset_lists_hop_nodes_unless_the_environment_names_others() {
		let previewnet = Network::Previewnet.config();
		assert_eq!(
			CliPersonChatHost::from_list(&previewnet, None).nodes,
			["wss://previewnet.substrate.dev/bulletin"]
		);
		// Paseo's Bulletin node does not serve HOP, so nothing is guessed.
		assert!(
			CliPersonChatHost::from_list(&Network::PaseoNextV2.config(), None)
				.nodes
				.is_empty()
		);
		assert_eq!(
			CliPersonChatHost::from_list(
				&previewnet,
				Some(" wss://a.example, ,wss://b.example".into())
			)
			.nodes,
			["wss://a.example", "wss://b.example"]
		);
	}
}
