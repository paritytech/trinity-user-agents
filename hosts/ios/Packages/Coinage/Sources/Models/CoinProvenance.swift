import Foundation

/// What a freshly minted coin inherits from the operation that produced it.
///
/// Passed to the minter rather than patched in afterwards: a coin is persisted the moment it is
/// allocated, so its provenance has to be known before the row exists.
public struct CoinProvenance: Equatable, Sendable {
    /// Fungibility of the recycler the coin ultimately came out of. `nil` when that is not known —
    /// nothing in a coin received from a peer reveals it.
    public let recyclerFungibility: UInt8?

    /// The operations that produced this coin, oldest first.
    public let hops: [Hop]

    public init(recyclerFungibility: UInt8?, hops: [Hop]) {
        self.recyclerFungibility = recyclerFungibility
        self.hops = hops
    }

    /// Straight out of a recycler: the coin's history starts here, so it carries no hops.
    public static func unloaded(recyclerFungibility: UInt8?) -> CoinProvenance {
        CoinProvenance(recyclerFungibility: recyclerFungibility, hops: [])
    }

    /// One of `fanout` pieces a coin was split into. Inherits the parent's chain and appends the
    /// split, so the whole provenance is preserved.
    public static func split(from parent: Coin, fanout: Int) -> CoinProvenance {
        CoinProvenance(
            recyclerFungibility: parent.recyclerFungibility,
            hops: parent.hops + [.split(fanout: UInt8(clamping: fanout))]
        )
    }

    /// Received from a peer. Their chain is not visible to us, so it is assumed to be one transfer
    /// per unit of on-chain age — the conservative reading — with the bundle we claimed it in
    /// recorded as the most recent hop.
    ///
    /// Recorded when the claim is submitted rather than when the coin is first seen on chain. The
    /// age is not a guess: the pallet stores `source age + 1` on a successful transfer, and a claim
    /// against a source that changed underneath us fails validation instead of succeeding with some
    /// other age. Recording it here is what preserves the real bundle size, which is not
    /// recoverable at first presence.
    ///
    /// The cost is that a claim which never settles leaves a coin carrying provenance for something
    /// that never existed on chain. Such a coin is not counted in the balance, so it is not shown.
    public static func received(ageAfterTransfer: Int16, bundleSize: Int) -> CoinProvenance {
        let hopCount = max(Int(ageAfterTransfer), 1)
        let unseen = Array(repeating: Hop.transfer(bundleSize: 1), count: hopCount - 1)

        return CoinProvenance(
            recyclerFungibility: nil,
            hops: unseen + [.transfer(bundleSize: UInt8(clamping: bundleSize))]
        )
    }

    /// Nothing is known: neither the recycler nor how the coin got here.
    public static let unknown = CoinProvenance(recyclerFungibility: nil, hops: [])
}
