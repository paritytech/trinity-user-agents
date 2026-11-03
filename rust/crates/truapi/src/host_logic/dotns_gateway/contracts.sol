// Solidity interfaces of the dotNS contracts the host reads, as far as it
// reads them. Signatures and docs mirror https://github.com/paritytech/dotns.

/// Dedicated PoP controller that issues device names and personhood names on
/// behalf of the dotNS gateway pallet.
///
/// [`DotnsPopController.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/registrars/DotnsPopController.sol)
interface DotnsPopController {
	/// Deferred per-user binding of a freshly minted name to its `LabelStore`.
	struct PendingClaim {
		/// Bare label without the TLD, which is appended at settlement time.
		string label;
		/// Timestamp of the originating mint.
		uint64 mintedAt;
	}

	/// Protocol-level address registry for all dotNS contracts.
	function protocolRegistry() external view returns (address);
	/// Whether this controller issued `label` as a PoP identity.
	function isPopIssued(string label) external view returns (bool);
	/// Returns a paginated slice of a user's pending claims in queue order.
	function pendingClaims(address user, uint256 offset, uint256 limit)
		external view returns (PendingClaim[]);
}

/// Non-upgradeable shim that translates a substrate Root-origin dispatch into
/// an EVM-observable authority on the PoP controller proxy.
///
/// [`RootGatewayDispatcher.sol`](https://github.com/paritytech/dotns/blob/38c4c7d4d87c4e8630727d6f7f4c27e30bd71248/contracts/registrars/RootGatewayDispatcher.sol)
interface RootGatewayDispatcher {
	/// Controller proxy address this dispatcher forwards to.
	function TARGET() external view returns (address);
}

/// Upgradeable address registry for all dotNS protocol contracts, and the
/// authority for the network's top-level domain.
///
/// [`DotnsProtocolRegistry.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/registry/DotnsProtocolRegistry.sol)
interface ProtocolRegistry {
	/// Returns the address stored for a given key.
	function get(bytes32 key) external view returns (address);
	/// Returns the network's TLD suffix, including the leading dot (e.g. `.dot`).
	function tld() external view returns (string);
}

/// Factory for the two per-user dotNS store types, sharing one factory
/// contract and two beacons.
///
/// [`StoreFactory.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/store/StoreFactory.sol)
interface StoreFactory {
	/// Returns the `LabelStore` address bound to `user`, or the zero address if none.
	function getLabelStore(address user) external view returns (address);
}

/// Permanent per-user dotNS label store.
///
/// [`LabelStore.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/store/LabelStore.sol)
interface LabelStore {
	/// Paginated read returning just the stored labels, in insertion order.
	function getLabels(uint256 offset, uint256 limit) external view returns (string[]);
}

/// ERC721-backed registrar implementing name ownership with no expiry.
///
/// [`DotnsRegistrar.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/registrars/DotnsRegistrar.sol)
interface DotnsRegistrar {
	/// Returns whether a given token id has been minted.
	function exists(uint256 tokenId) external view returns (bool);
	/// Returns the owner of the `tokenId` token.
	function ownerOf(uint256 tokenId) external view returns (address);
}

/// Upgradeable on-chain registry for hierarchical name ownership and resolution.
///
/// [`DotnsRegistry.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/registry/DotnsRegistry.sol)
interface DotnsRegistry {
	/// Returns whether a node exists and resolves.
	function recordExists(bytes32 node) external view returns (bool);
	/// Returns the resolver of a node, or zero when the node does not resolve.
	function resolver(bytes32 node) external view returns (address);
}

/// Implements `IDotnsContentResolver` interface with content hash, text
/// records, and operator approvals.
///
/// [`DotnsContentResolver.sol`](https://github.com/paritytech/dotns/blob/94a778e0d1187ee6c0c199378cae8d916af6ad68/contracts/resolvers/DotnsContentResolver.sol)
interface ContentResolver {
	/// Returns a text record for a node.
	function text(bytes32 node, string key) external view returns (string);
}
