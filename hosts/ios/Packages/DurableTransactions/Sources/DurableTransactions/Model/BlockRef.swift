import Foundation

/// A block identified by number and hash together, so the two can never drift apart.
///
/// Used for a transaction's checkpoint and for the block where its execution was first observed.
public struct BlockRef: Hashable, Sendable {
    public let number: UInt32
    public let hash: Data

    public init(number: UInt32, hash: Data) {
        self.number = number
        self.hash = hash
    }
}

/// The finalized and best heads a single recovery pass evaluates against.
///
/// Both heads are pinned once, from one connection, before the pass reads anything, so every rule in the
/// pass sees the same chain. Freshness comes from re-pinning a new view each pass: a read is addressed by
/// block hash, so a swapped connection yields a failed read rather than a wrong verdict.
public struct ChainHeads: Sendable, Equatable {
    public let finalized: BlockRef
    public let best: BlockRef

    public init(finalized: BlockRef, best: BlockRef) {
        self.finalized = finalized
        self.best = best
    }
}
