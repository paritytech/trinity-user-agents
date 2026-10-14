import AsyncExtensions
import Foundation
import SubstrateSdk

/// What a block's events say about one extrinsic's dispatch.
///
/// `reason` is the chain's own name for the failure (`"Coinage.RingRevisionMismatch"`), decoded from the
/// `ExtrinsicFailed` event for diagnostics only: a verdict rests on the dispatch having failed, never on
/// being able to name why, so a reason that cannot be decoded is `nil` rather than a failed read.
public enum DispatchOutcome: Sendable, Equatable {
    case succeeded
    case failed(reason: String?)
}

/// Result of scanning the mortality window for an extrinsic hash.
public enum BodySearchOutcome: Sendable, Equatable {
    case foundSucceeded(BlockRef)
    case foundFailed(BlockRef, reason: String?)
    /// Included, but the events at that block could not be decoded.
    case foundOutcomeUnreadable(BlockRef)
    /// Every block in the window was read and the hash is in none of them.
    case notFoundWindowComplete
    /// At least one block in the window could not be read, so the search decided nothing.
    case incomplete
}

/// Outcome of looking one extrinsic hash up in one block.
///
/// `unreadable` is never proof of absence — only `notInBlock` is, and only because the block was read
/// in full.
public enum BlockLookup: Sendable {
    case unreadable
    case notInBlock
    case outcome(ReadResult<DispatchOutcome>)
}

/// Reads one extrinsic hash's dispatch outcome from one block. The single access that needs a live
/// connection and runtime metadata, behind a protocol so the window-scan logic in ``BlockBodyScan`` can
/// be exercised without either.
public protocol BlockOutcomeReading: Sendable {
    func lookUp(_ txHash: Data, at blockHash: Data) async -> BlockLookup
}

/// One view of the chain, pinned for the length of a single recovery pass.
///
/// These are the reads that are true of any transaction: where a block is, what is in its body, and
/// whether a dispatch succeeded. Anything domain-shaped is read by the domain's own
/// ``TxCompletionOracle``, at the heads this view pins.
///
/// Every method returns `failedRead` rather than throwing on transport failure, an unknown block or an
/// undecodable value, so a read failure can never be mistaken for absence.
public protocol PinnedChainViewProtocol: Sendable {
    var chainId: ChainId { get }

    /// The finalized head this view is pinned to.
    var finalizedHead: BlockRef { get }

    /// The best head this view is pinned to.
    var bestHead: BlockRef { get }

    /// Canonical hash at a block number. `absent` when the chain has no block at that height.
    func blockHash(at number: UInt32) async -> ReadResult<Data>

    /// Resolves a block hash to a full ``BlockRef``.
    func blockRef(forHash hash: Data) async -> ReadResult<BlockRef>

    /// Reads the dispatch outcome of `txHash` from the events at `block`: `ExtrinsicSuccess` or
    /// `ExtrinsicFailed`, the latter carrying the decoded failure name when the events yield one.
    func dispatchOutcome(txHash: Data, at block: BlockRef) async -> ReadResult<DispatchOutcome>

    /// Scans `window` for `txHash` and, on a hit, reads the dispatch outcome from the same block the
    /// extrinsic was found in. Callers bound the window at the finalized head, so a hit is always
    /// finalized and a terminal verdict from it rests on a finalized fact.
    func searchBodies(for txHash: Data, in window: ClosedRange<UInt32>) async -> BodySearchOutcome
}

public extension PinnedChainViewProtocol {
    var heads: ChainHeads {
        ChainHeads(finalized: finalizedHead, best: bestHead)
    }
}

/// Pins one ``PinnedChainViewProtocol`` per chain per pass, and owns the head subscriptions that drive
/// recovery.
public protocol PinnedChainViewFactoryProtocol: Sendable {
    /// Reads the finalized and best heads once and returns a view pinned to them. Throws when the best
    /// head is below finality — the peer is inconsistent and the pass must not run against it.
    func pin(chainId: ChainId) async throws -> any PinnedChainViewProtocol

    /// Every newly finalized block, so recovery runs a pass exactly when the facts it reads can have
    /// changed. The number is a tick — each pass still pins its own view.
    func finalizedHeads(chainId: ChainId) -> AnyAsyncSequence<BlockNumber>

    /// Every new best block. Pre-finality success is read at the best head, so those facts move here
    /// rather than at finality — several blocks earlier.
    func bestHeads(chainId: ChainId) -> AnyAsyncSequence<BlockNumber>
}
