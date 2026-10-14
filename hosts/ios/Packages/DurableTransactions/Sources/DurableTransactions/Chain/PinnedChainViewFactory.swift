import AsyncAlgorithms
@preconcurrency import AsyncExtensions
import ChainStore
@preconcurrency import ExtrinsicService
import Foundation
import Operation_iOS
import os
@preconcurrency import SDKLogger
import StructuredConcurrency
import SubstrateOperation
@preconcurrency import SubstrateSdk
import SubstrateSdkExt
import SubstrateStorageQuery

/// Builds a ``PinnedChainView`` per chain from the chain resource, and owns the resilient head
/// subscriptions that drive recovery. Per-chain readers are created once and reused across passes.
public final class PinnedChainViewFactory: PinnedChainViewFactoryProtocol, @unchecked Sendable {
    private let chainResource: any ChainResourceProtocol
    private let operationQueue: OperationQueue
    private let logger: SDKLoggerProtocol?
    private let readers = OSAllocatedUnfairLock<[ChainId: ChainReaders]>(initialState: [:])

    /// Reconnect attempts before a head subscription gives up and pauses until the next `start()`.
    private static let maxSubscribeAttempts = 5

    public init(chainResource: any ChainResourceProtocol, operationQueue: OperationQueue, logger: SDKLoggerProtocol?) {
        self.chainResource = chainResource
        self.operationQueue = operationQueue
        self.logger = logger
    }

    /// The connection-bound readers of one chain.
    private struct ChainReaders {
        let blockInfoProvider: any BlockInfoProviding
        let outcomeReader: BlockOutcomeReader
    }
}

// MARK: - Pinning

extension PinnedChainViewFactory {
    public func pin(chainId: ChainId) async throws -> any PinnedChainViewProtocol {
        let readers = try readers(for: chainId)
        let blockInfoProvider = readers.blockInfoProvider

        let finalizedNumber = try await blockInfoProvider.fetchFinalized()
        let finalizedHash = try await blockInfoProvider.fetchBlockHash(finalizedNumber)
        let bestNumber = try await blockInfoProvider.fetchCurrent()
        let bestHash = try await blockInfoProvider.fetchBlockHash(bestNumber)

        guard bestNumber >= finalizedNumber else {
            throw DurableTxError.chainViewUnavailable
        }

        return PinnedChainView(
            chainId: chainId,
            heads: ChainHeads(
                finalized: BlockRef(number: finalizedNumber, hash: finalizedHash),
                best: BlockRef(number: bestNumber, hash: bestHash)
            ),
            blockInfoProvider: blockInfoProvider,
            outcomeReader: readers.outcomeReader
        )
    }

    private func readers(for chainId: ChainId) throws -> ChainReaders {
        if let cached = readers.withLock({ $0[chainId] }) {
            return cached
        }

        guard
            let connection = chainResource.getRpcConnection(for: chainId),
            let runtimeService = chainResource.getRuntimeCodingService(for: chainId)
        else {
            throw DurableTxError.chainViewUnavailable
        }

        let built = ChainReaders(
            blockInfoProvider: BlockInfoProvider(
                chainRegistry: chainResource,
                operationQueue: operationQueue,
                chainId: chainId
            ),
            outcomeReader: BlockOutcomeReader(
                connection: connection,
                runtimeService: runtimeService,
                eventsQueryFactory: BlockEventsQueryFactory(
                    operationQueue: operationQueue,
                    eventsRepository: SubstrateEventsRepository(),
                    storageRequestFactory: StorageRequestFactory(
                        remoteFactory: StorageKeyFactory(),
                        operationManager: OperationManager(operationQueue: operationQueue)
                    ),
                    logger: logger
                ),
                errorDecoder: CallDispatchErrorDecoder(logger: logger)
            )
        )

        return readers.withLock { current in
            if let existing = current[chainId] { return existing }
            current[chainId] = built
            return built
        }
    }
}

// MARK: - Head subscriptions

public extension PinnedChainViewFactory {
    func finalizedHeads(chainId: ChainId) -> AnyAsyncSequence<BlockNumber> {
        headStream(chainId: chainId) { $0.subscribeFinalizedHeads() }
    }

    func bestHeads(chainId: ChainId) -> AnyAsyncSequence<BlockNumber> {
        headStream(chainId: chainId) { $0.subscribeNewHeads() }
    }

    /// Wraps `subscribe` in a resilient loop that resubscribes across drops and dedups heads within one
    /// subscription, yielding each head's number as a tick. A chain whose readers cannot be built yields
    /// nothing: the pass that follows would not be able to pin it either.
    private func headStream(
        chainId: ChainId,
        subscribe: @escaping @Sendable (any BlockInfoProviding) -> AnyAsyncSequence<Block.Header>
    ) -> AnyAsyncSequence<BlockNumber> {
        guard let blockInfoProvider = try? readers(for: chainId).blockInfoProvider else {
            logger?.error("No chain readers for \(chainId), head stream is empty")
            return AsyncStream<BlockNumber> { $0.finish() }.eraseToAnyAsyncSequence()
        }

        return AsyncStream<BlockNumber> { continuation in
            let task = Task { [logger] in
                while !Task.isCancelled {
                    do {
                        let heads = try await withRetry(
                            maxAttempts: Self.maxSubscribeAttempts,
                            initialDelay: .seconds(1)
                        ) {
                            let sequence = subscribe(blockInfoProvider)
                            var iterator = sequence.makeAsyncIterator()
                            guard let first = try await iterator.next() else {
                                throw HeadSubscriptionError.subscriptionEndedBeforeFirstHead
                            }
                            return PrefixedHeadSequence(firstElement: first, iterator: iterator)
                                .removeDuplicates { $0.number == $1.number }
                                .eraseToAnyAsyncSequence()
                        }

                        for try await head in heads {
                            guard !Task.isCancelled else { break }
                            if let number = UInt32.fromHex(head.number) {
                                continuation.yield(number)
                            }
                        }
                    } catch RetryError.limitReached {
                        logger?.error("Head subscription exhausted retries, pausing")
                        break
                    } catch {
                        logger?.error("Head subscription failed: \(error), reconnecting")
                    }
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }
}

private enum HeadSubscriptionError: Error {
    /// The subscription ended before yielding any head.
    case subscriptionEndedBeforeFirstHead
}

// MARK: - Block-events read

/// Looks one extrinsic hash up in one block and resolves its dispatch outcome from that same block's
/// events. Isolates the one access that needs a live connection and runtime metadata.
final class BlockOutcomeReader: BlockOutcomeReading {
    private let connection: any JSONRPCEngine
    private let runtimeService: any RuntimeCodingServiceProtocol
    private let eventsQueryFactory: any BlockEventsQueryFactoryProtocol
    private let errorDecoder: any CallDispatchErrorDecoding

    init(
        connection: any JSONRPCEngine,
        runtimeService: any RuntimeCodingServiceProtocol,
        eventsQueryFactory: any BlockEventsQueryFactoryProtocol,
        errorDecoder: any CallDispatchErrorDecoding
    ) {
        self.connection = connection
        self.runtimeService = runtimeService
        self.eventsQueryFactory = eventsQueryFactory
        self.errorDecoder = errorDecoder
    }

    func lookUp(_ txHash: Data, at blockHash: Data) async -> BlockLookup {
        guard let blockDetails = try? await queryBlockDetails(blockHash: blockHash) else {
            return .unreadable
        }

        guard let extrinsic = blockDetails.extrinsicsWithEvents.first(where: { $0.extrinsicHash == txHash }) else {
            return .notInBlock
        }

        return await resolveOutcome(extrinsic: extrinsic)
    }
}

private extension BlockOutcomeReader {
    func queryBlockDetails(blockHash: Data) async throws -> SubstrateBlockDetails {
        let wrapper = eventsQueryFactory.queryBlockDetailsWrapper(
            from: connection,
            runtimeProvider: runtimeService,
            blockHash: blockHash
        )
        return try await wrapper.asyncExecute()
    }

    func resolveOutcome(extrinsic: SubstrateExtrinsicEvents) async -> BlockLookup {
        guard let coderFactory = try? await runtimeService.fetchCoderFactoryOperation().asyncExecute() else {
            return .outcome(.failedRead)
        }

        let successMatcher = ExtrinsicSuccessEventMatcher()
        let failureMatcher = ExtrinsicFailureEventMatcher()

        for record in extrinsic.eventRecords {
            if successMatcher.match(event: record.event, using: coderFactory) {
                return .outcome(.present(.succeeded))
            }
            if failureMatcher.match(event: record.event, using: coderFactory) {
                let reason = errorDecoder
                    .decode(errorParams: record.event.params, using: coderFactory)
                    .map(Self.describe)

                return .outcome(.present(.failed(reason: reason)))
            }
        }

        // Applied, but neither outcome event is present — the block was read and still says nothing.
        return .outcome(.failedRead)
    }

    /// The chain's own name for the failure where the metadata yields one, falling back to the raw
    /// module and error indices so an undecodable variant still identifies itself in a log.
    static func describe(_ error: Substrate.DispatchCallError) -> String {
        switch error {
        case let .module(moduleError):
            "\(moduleError.display.moduleName).\(moduleError.display.errorName)"
        case let .other(other):
            [other.module, other.reason].compactMap { $0 }.joined(separator: ".")
        }
    }
}

// MARK: - Head subscription wrapper

/// Wraps an iterator to re-yield its first element plus all subsequent elements.
///
/// Declared `@unchecked Sendable` because `AnyAsyncIterator<Block.Header>` is not `Sendable`. The
/// iterator is consumed within a single uncontended async task; no concurrent access occurs.
private struct PrefixedHeadSequence: AsyncSequence, @unchecked Sendable {
    typealias Element = Block.Header

    let firstElement: Block.Header
    var iterator: AnyAsyncIterator<Block.Header>

    func makeAsyncIterator() -> Iterator {
        Iterator(firstElement: firstElement, iterator: iterator, yieldedFirst: false)
    }

    struct Iterator: AsyncIteratorProtocol, @unchecked Sendable {
        typealias Element = Block.Header

        let firstElement: Block.Header
        var iterator: AnyAsyncIterator<Block.Header>
        var yieldedFirst: Bool

        mutating func next() async throws -> Block.Header? {
            if !yieldedFirst {
                yieldedFirst = true
                return firstElement
            }
            return try await iterator.next()
        }
    }
}
