//! Solidity interfaces of the dotNS contracts the host reads, as far as it
//! reads them. Signatures mirror `paritytech/dotns`.

alloy_sol_types::sol! {
	/// The gateway's proof-of-personhood controller.
	interface DotnsPopController {
		/// A gateway-minted label waiting for `claimLabelStore`.
		struct PendingClaim {
			/// The bare label.
			string label;
			/// Unix seconds of the mint.
			uint64 mintedAt;
		}

		/// The protocol registry the controller was deployed against.
		function protocolRegistry() external view returns (address);
		/// Whether the controller issued `label`.
		function isPopIssued(string label) external view returns (bool);
		/// One page of `user`'s pending claims.
		function pendingClaims(address user, uint256 offset, uint256 limit)
			external view returns (PendingClaim[]);
	}

	/// The Root-gated dispatcher a chain may store in front of the controller.
	interface RootGatewayDispatcher {
		/// The controller the dispatcher forwards to.
		function TARGET() external view returns (address);
	}

	/// The address book of the dotNS deployment.
	interface ProtocolRegistry {
		/// The component registered under `key`, a `bytes32("name")` literal.
		function get(bytes32 key) external view returns (address);
		/// The network TLD with its leading dot.
		function tld() external view returns (string);
	}

	/// Deploys and locates per-account label stores.
	interface StoreFactory {
		/// `user`'s label store, or the zero address when none is deployed.
		function getLabelStore(address user) external view returns (address);
	}

	/// The append-only list of labels written for one account.
	interface LabelStore {
		/// One page of labels, each carrying the network TLD.
		function getLabels(uint256 offset, uint256 limit) external view returns (string[]);
	}

	/// The ERC-721 registrar whose token ids are name nodes.
	interface DotnsRegistrar {
		/// Whether the name with node `tokenId` is minted.
		function exists(uint256 tokenId) external view returns (bool);
		/// The holder of the name with node `tokenId`.
		function ownerOf(uint256 tokenId) external view returns (address);
	}

	/// The ENS-style registry of name records.
	interface DotnsRegistry {
		/// Whether `node` has a record.
		function recordExists(bytes32 node) external view returns (bool);
		/// The resolver serving `node`, or the zero address.
		function resolver(bytes32 node) external view returns (address);
	}

	/// A resolver serving text records.
	interface ContentResolver {
		/// The text record `key` of `node`.
		function text(bytes32 node, string key) external view returns (string);
	}
}
