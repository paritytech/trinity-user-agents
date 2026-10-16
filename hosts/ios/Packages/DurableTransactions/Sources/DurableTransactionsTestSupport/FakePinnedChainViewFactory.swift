import AsyncExtensions
import DurableTransactions
import Foundation
import os
import SubstrateSdk

/// Which engine-shaped reads fail, so a scenario can hold a transaction undecided without changing what
/// the chain holds. Domain reads carry their own faults on the domain's fake reader.
public struct FakeChainFaults {
    public var unreadableBlocks: Set<UInt32> = []
    /// A standing rule rather than a set, so it covers blocks produced after it was switched on.
    public var everyBlockUnreadable = false
    public var unreadableOutcomes: Set<Data> = []
    public var pinFails = false
    /// The body search reads nothing, so it can never decide a transaction. Separate from
    /// `everyBlockUnreadable`, which also takes out the block reads registration and pinning need.
    public var txSearchDisabled = false

    public init() {}

    public static let none = FakeChainFaults()
}

/// Raised by the fake chain view when a fault silences a read.
public struct ChainReadFailure: Error {
    public let message: String

    public init(message: String) {
        self.message = message
    }
}

/// Drives a ``FakeChain`` and hands out views over it, standing in for the engine's chain half. Every
/// read honours the ``PinnedChainViewProtocol`` contract: a failure mode becomes `failedRead`, never
/// `absent`.
///
/// `faults` is mutable so a single-pass fault can be switched on, a pass run, then switched off — each
/// pinned view reads the faults in force at read time.
public final class FakePinnedChainViewFactory<State: FakeChainState>: PinnedChainViewFactoryProtocol,
    @unchecked Sendable {
    public let chain: FakeChain<State>

    // Trackers pin from concurrent tasks while scenarios switch faults, so all mutable state is locked.
    private struct MutableState {
        var faults: FakeChainFaults = .none
        var pins = 0
        var pinnedChainIds: [ChainId] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: MutableState())

    public var faults: FakeChainFaults {
        get { state.withLock { $0.faults } }
        set { state.withLock { $0.faults = newValue } }
    }

    /// How many times a view was pinned, so a scenario can assert a pass did not read the chain.
    public var pins: Int { state.withLock { $0.pins } }

    /// The chain ids pins were asked for, in order.
    public var pinnedChainIds: [ChainId] { state.withLock { $0.pinnedChainIds } }

    // Never finishes on its own, like the production stream.
    private let finalizedHeadTicks = AsyncPassthroughSubject<BlockNumber>()

    public init(chain: FakeChain<State>) {
        self.chain = chain
    }

    public func emitFinalizedHead(_ number: BlockNumber) {
        finalizedHeadTicks.send(number)
    }

    public func pin(chainId: ChainId) async throws -> any PinnedChainViewProtocol {
        let currentFaults = state.withLock { current in
            current.pins += 1
            current.pinnedChainIds.append(chainId)
            return current.faults
        }

        if currentFaults.pinFails {
            throw ChainReadFailure(message: "pin failed")
        }

        let finalized = chain.finalizedHead
        let best = chain.bestHead
        return FakePinnedChainView(
            chainId: chainId,
            chain: chain,
            // Read live, not snapshotted at pin: a fault switched on after a view is pinned (a watcher
            // scenario blinding a block mid-flight) is a read-time transport failure, so it must reach a
            // read this view makes later.
            faults: { [self] in faults },
            finalized: BlockRef(number: finalized.number, hash: finalized.hash),
            best: BlockRef(number: best.number, hash: best.hash)
        )
    }

    public func finalizedHeads(chainId _: ChainId) -> AnyAsyncSequence<BlockNumber> {
        finalizedHeadTicks.eraseToAnyAsyncSequence()
    }

    public func bestHeads(chainId _: ChainId) -> AnyAsyncSequence<BlockNumber> {
        AsyncStream<BlockNumber> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}

/// One pinned view over the fake chain, bound to the two heads read when it was pinned.
private final class FakePinnedChainView<State: FakeChainState>: PinnedChainViewProtocol, @unchecked Sendable {
    let chainId: ChainId
    let finalizedHead: BlockRef
    let bestHead: BlockRef

    private let chain: FakeChain<State>
    private let faultsProvider: () -> FakeChainFaults
    private var faults: FakeChainFaults { faultsProvider() }

    init(
        chainId: ChainId,
        chain: FakeChain<State>,
        faults: @escaping () -> FakeChainFaults,
        finalized: BlockRef,
        best: BlockRef
    ) {
        self.chainId = chainId
        self.chain = chain
        faultsProvider = faults
        finalizedHead = finalized
        bestHead = best
    }

    func blockHash(at number: UInt32) async -> ReadResult<Data> {
        if faults.everyBlockUnreadable || faults.unreadableBlocks.contains(number) {
            return .failedRead
        }
        guard let block = chain.canonicalAt(number) else { return .absent }
        return .present(block.hash)
    }

    func blockRef(forHash hash: Data) async -> ReadResult<BlockRef> {
        guard let block = chain.blockAt(hash: hash) else { return .failedRead }
        if faults.everyBlockUnreadable || faults.unreadableBlocks.contains(block.number) {
            return .failedRead
        }
        return .present(BlockRef(number: block.number, hash: block.hash))
    }

    func dispatchOutcome(txHash: Data, at block: BlockRef) async -> ReadResult<DispatchOutcome> {
        switch lookUp(txHash, atBlockHash: block.hash) {
        case let .outcome(result): result
        case .notInBlock,
             .unreadable: .failedRead
        }
    }

    /// Newest block first, mirroring the production ``BlockBodyScan``: a hit reads the dispatch outcome
    /// from the same block; a window not read in full decides nothing.
    func searchBodies(for txHash: Data, in window: ClosedRange<UInt32>) async -> BodySearchOutcome {
        if faults.txSearchDisabled { return .incomplete }

        var everyBlockRead = true
        for number in window.reversed() {
            guard let candidate = chain.canonicalAt(number),
                  !faults.everyBlockUnreadable,
                  !faults.unreadableBlocks.contains(number)
            else {
                everyBlockRead = false
                continue
            }

            let ref = BlockRef(number: number, hash: candidate.hash)
            switch lookUp(txHash, atBlockHash: candidate.hash) {
            case .unreadable:
                everyBlockRead = false
            case .notInBlock:
                continue
            case let .outcome(result):
                return Self.mapSearchOutcome(result: result, block: ref)
            }
        }

        return everyBlockRead ? .notFoundWindowComplete : .incomplete
    }
}

private extension FakePinnedChainView {
    func lookUp(_ txHash: Data, atBlockHash blockHash: Data) -> BlockLookup {
        guard let block = chain.blockAt(hash: blockHash) else { return .unreadable }
        if faults.unreadableOutcomes.contains(txHash) { return .unreadable }
        if faults.everyBlockUnreadable || faults.unreadableBlocks.contains(block.number) {
            return .unreadable
        }
        guard block.body.contains(txHash) else { return .notInBlock }

        guard let success = block.state.outcomes[txHash] else { return .outcome(.failedRead) }
        return .outcome(.present(success ? .succeeded : .failed(reason: "Fake.DispatchFailed")))
    }

    static func mapSearchOutcome(result: ReadResult<DispatchOutcome>, block: BlockRef) -> BodySearchOutcome {
        switch result {
        case .present(.succeeded): .foundSucceeded(block)
        case let .present(.failed(reason)): .foundFailed(block, reason: reason)
        case .absent,
             .failedRead: .foundOutcomeUnreadable(block)
        }
    }
}
