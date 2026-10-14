import AsyncExtensions
import DurableTransactions
import Foundation
import os
import SubstrateSdk

/// A test double that is both a ``PinnedChainViewProtocol`` (a pinned view) and its factory
/// (self-pinning: `pin(chainId:)` returns itself), so a test injects one object wherever the engine
/// expects either.
///
/// Default behaviour for any unconfigured read is `absent`; tests must opt in to presence. The pinned
/// heads are whatever the test last set via ``setHeads(finalized:best:)``.
public final class StubPinnedChainView: PinnedChainViewProtocol, PinnedChainViewFactoryProtocol, @unchecked Sendable {
    public let chainId: ChainId

    private struct State {
        var heads: ChainHeads
        var hashResults: [UInt32: ReadResult<Data>] = [:]
        var refResults: [Data: ReadResult<BlockRef>] = [:]
        var outcomeResults: [UInt32: [Data: ReadResult<DispatchOutcome>]] = [:]
        var bodySearchResponses: [Data: BodySearchOutcome] = [:]
        var pinCount = 0
        var pinFails = false
        var searchedHashes: [Data] = []
        var searchedWindows: [Data: ClosedRange<UInt32>] = [:]
    }

    private let state: OSAllocatedUnfairLock<State>

    public init(
        chainId: ChainId = "stub-chain",
        finalized: BlockRef = BlockRef(number: 100, hash: Data([100])),
        best: BlockRef = BlockRef(number: 110, hash: Data([110]))
    ) {
        self.chainId = chainId
        state = OSAllocatedUnfairLock(initialState: State(heads: ChainHeads(finalized: finalized, best: best)))
    }

    public var finalizedHead: BlockRef { state.withLock { $0.heads.finalized } }
    public var bestHead: BlockRef { state.withLock { $0.heads.best } }

    // MARK: - Configuration

    public func setHeads(finalized: BlockRef, best: BlockRef) {
        state.withLock { $0.heads = ChainHeads(finalized: finalized, best: best) }
    }

    public func setBlockHash(_ hash: Data, forNumber number: UInt32) {
        state.withLock { $0.hashResults[number] = .present(hash) }
    }

    public func setBlockHashFailed(_ number: UInt32) {
        state.withLock { $0.hashResults[number] = .failedRead }
    }

    public func setBlockRef(_ ref: BlockRef) {
        state.withLock { $0.refResults[ref.hash] = .present(ref) }
    }

    public func setBlockRefFailed(_ hash: Data) {
        state.withLock { $0.refResults[hash] = .failedRead }
    }

    public func setDispatchOutcome(_ txHash: Data, at block: BlockRef, success: Bool) {
        let outcome: DispatchOutcome = success ? .succeeded : .failed(reason: "Stub.DispatchFailed")
        state.withLock { $0.outcomeResults[block.number, default: [:]][txHash] = .present(outcome) }
    }

    public func setDispatchOutcomeFailed(_ txHash: Data, at block: BlockRef) {
        state.withLock { $0.outcomeResults[block.number, default: [:]][txHash] = .failedRead }
    }

    public func setBodySearchResponse(_ txHash: Data, to outcome: BodySearchOutcome) {
        state.withLock { $0.bodySearchResponses[txHash] = outcome }
    }

    public func setPinFails(_ fails: Bool) {
        state.withLock { $0.pinFails = fails }
    }

    // MARK: - Observations

    public var pinCount: Int { state.withLock { $0.pinCount } }

    /// The hashes the body search was asked for, in order — the ladder reached its last rule for them.
    public var searchedHashes: [Data] { state.withLock { $0.searchedHashes } }

    public func searchedWindow(for txHash: Data) -> ClosedRange<UInt32>? {
        state.withLock { $0.searchedWindows[txHash] }
    }

    // MARK: - PinnedChainViewFactoryProtocol

    public func pin(chainId _: ChainId) async throws -> any PinnedChainViewProtocol {
        let fails = state.withLock { current -> Bool in
            current.pinCount += 1
            return current.pinFails
        }
        if fails { throw ChainReadFailure(message: "pin failed") }
        return self
    }

    public func finalizedHeads(chainId _: ChainId) -> AnyAsyncSequence<BlockNumber> {
        AsyncStream<BlockNumber> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    public func bestHeads(chainId _: ChainId) -> AnyAsyncSequence<BlockNumber> {
        AsyncStream<BlockNumber> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    // MARK: - PinnedChainViewProtocol

    public func blockHash(at number: UInt32) async -> ReadResult<Data> {
        state.withLock { $0.hashResults[number] ?? .absent }
    }

    public func blockRef(forHash hash: Data) async -> ReadResult<BlockRef> {
        state.withLock { $0.refResults[hash] ?? .absent }
    }

    public func dispatchOutcome(txHash: Data, at block: BlockRef) async -> ReadResult<DispatchOutcome> {
        state.withLock { ($0.outcomeResults[block.number] ?? [:])[txHash] ?? .absent }
    }

    public func searchBodies(for txHash: Data, in window: ClosedRange<UInt32>) async -> BodySearchOutcome {
        state.withLock { current in
            current.searchedHashes.append(txHash)
            current.searchedWindows[txHash] = window
            return current.bodySearchResponses[txHash] ?? .notFoundWindowComplete
        }
    }
}
