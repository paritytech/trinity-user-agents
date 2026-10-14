import DurableTransactions
import Foundation

/// The per-block state a ``FakeChain`` carries. Only the dispatch outcomes are engine-shaped; a domain
/// keeps whatever else it reads in its own conforming type.
public protocol FakeChainState {
    /// Dispatch outcome of each extrinsic applied in this block: `true` success, `false` failure.
    /// Per-block rather than cumulative, so a transaction reorged out of one block and re-applied in
    /// another carries the outcome of the block it is read at.
    var outcomes: [Data: Bool] { get }
}

/// One block: its number and hash, the transactions it carries, and the full chain state as of it.
public struct FakeBlock<State: FakeChainState> {
    public let number: UInt32
    public let hash: Data
    public let parentHash: Data
    public let state: State
    public let body: [Data]
}

/// A block tree with per-block state, finalization and reorgs.
///
/// Reorged-out blocks stay readable by hash, the way a node still serves an orphaned block; only the
/// canonical chain moves. `reorgDepths` is bounded by the unfinalized suffix, so a finalized block cannot
/// be reorged and no failure a fuzzer finds is merely a violated requirement.
///
/// Every produced block gets a fresh, unique hash, so a reorg followed by a replacing branch of the same
/// height still changes the canonical hash at that height — which is exactly what lets a recorded block's
/// canonicality flip to false.
public final class FakeChain<State: FakeChainState>: @unchecked Sendable {
    private var byHash: [Data: FakeBlock<State>] = [:]
    private var canonical: [FakeBlock<State>] = []
    private var nextHashSeq: UInt64 = 0
    private var finalizedNumber: UInt32 = 0

    public init(initialState: State) {
        let genesis = FakeBlock(
            number: 0,
            hash: freshHash(),
            parentHash: Data(),
            state: initialState,
            body: []
        )
        byHash[genesis.hash] = genesis
        canonical.append(genesis)
    }

    public var bestHead: FakeBlock<State> { canonical[canonical.count - 1] }

    public var finalizedHead: FakeBlock<State> { canonical[Int(finalizedNumber)] }

    /// Empty when nothing is unfinalized, which is what makes a finalized block unreorgable.
    public var reorgDepths: [Int] {
        let span = Int(bestHead.number) - Int(finalizedNumber)
        guard span >= 1 else { return [] }
        return Array(1 ... span)
    }

    @discardableResult
    public func produceBlock(body: [Data] = [], mutate: (State) -> State = { $0 }) -> FakeBlock<State> {
        let parent = bestHead
        let block = FakeBlock(
            number: parent.number + 1,
            hash: freshHash(),
            parentHash: parent.hash,
            state: mutate(parent.state),
            body: body
        )
        byHash[block.hash] = block
        canonical.append(block)
        return block
    }

    public func finalize(upTo number: UInt32) {
        let clamped = min(number, bestHead.number)
        finalizedNumber = max(finalizedNumber, clamped)
    }

    /// Rewinds the canonical head by `depth`; the caller produces the replacing branch. The dropped
    /// blocks stay readable by hash.
    public func reorg(depth: Int) {
        guard reorgDepths.contains(depth) else { return }
        canonical.removeLast(depth)
    }

    public func stateAt(hash: Data) -> State? { byHash[hash]?.state }

    public func blockAt(hash: Data) -> FakeBlock<State>? { byHash[hash] }

    public func canonicalAt(_ number: UInt32) -> FakeBlock<State>? {
        Int(number) < canonical.count ? canonical[Int(number)] : nil
    }

    private func freshHash() -> Data {
        defer { nextHashSeq += 1 }
        var bytes = [UInt8](repeating: 0, count: 32)
        var seq = nextHashSeq
        for offset in 0 ..< 8 {
            bytes[31 - offset] = UInt8(truncatingIfNeeded: seq)
            seq >>= 8
        }
        return Data(bytes)
    }
}

/// The minimal state for a domain-free chain: dispatch outcomes only.
public struct OutcomesOnlyChainState: FakeChainState {
    public var outcomes: [Data: Bool]

    public init(outcomes: [Data: Bool] = [:]) {
        self.outcomes = outcomes
    }

    public static let empty = OutcomesOnlyChainState()

    public func applied(_ txHash: Data, success: Bool) -> Self {
        var next = self
        next.outcomes[txHash] = success
        return next
    }
}
