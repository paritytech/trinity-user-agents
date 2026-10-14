import Foundation
import AsyncExtensions
import BigInt
import ChainRegistry
import Operation_iOS
import StructuredConcurrency
import SubstrateOperation
import SubstrateSdk

struct ChainBlockInfo: Equatable {
    let number: BlockNumber
    let receivedAt: Date
    let finalizedNumber: BlockNumber?
}

protocol ChainBlockProviding: Actor {
    nonisolated func blockStream() -> AnyAsyncSequence<[ChainConnectionTarget: ChainBlockInfo]>
    func setActive(_ isActive: Bool)
    func clear(for target: ChainConnectionTarget)
}

/// Per-chain best block height, subscribed only while a host asks for it.
///
/// Sibling to `ChainStatusProvider` rather than part of it: this owns head subscriptions only, so
/// row composition stays in one place.
actor ChainBlockProvider {
    private let blocksSubject = AsyncCurrentValueSubject<[ChainConnectionTarget: ChainBlockInfo]>([:])

    private let blockProviders: [ChainConnectionTarget: BlockInfoProviding]
    private let logger: LoggerProtocol

    private var blocks: [ChainConnectionTarget: ChainBlockInfo] = [:]
    private var finalizedNumbers: [ChainConnectionTarget: BlockNumber] = [:]
    private var subscriptionTasks: [Task<Void, Never>] = []
    private var headsStreamYielded: Set<ChainConnectionTarget> = []
    private var finalizedStreamYielded: Set<ChainConnectionTarget> = []
    private var isActive = false

    private static let initialReconnectDelay: Duration = .seconds(1)
    private static let maxReconnectDelay: Duration = .seconds(30)
    private static let initialFetchAttempts = 5

    init(
        chainRegistry: ChainRegistryProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.logger = logger

        blockProviders = ChainConnectionTarget.allCases
            .reduce(into: [ChainConnectionTarget: BlockInfoProviding]()) { accumulator, target in
                accumulator[target] = BlockInfoProvider(
                    chainRegistry: chainRegistry,
                    operationQueue: operationQueue,
                    chainId: target.chainId
                )
            }
    }

    deinit {
        subscriptionTasks.forEach { $0.cancel() }
    }
}

extension ChainBlockProvider: ChainBlockProviding {
    nonisolated func blockStream() -> AnyAsyncSequence<[ChainConnectionTarget: ChainBlockInfo]> {
        blocksSubject.eraseToAnyAsyncSequence()
    }

    func setActive(_ isActive: Bool) {
        guard isActive != self.isActive else {
            return
        }

        self.isActive = isActive

        guard isActive else {
            subscriptionTasks.forEach { $0.cancel() }
            subscriptionTasks = []
            // Values are kept so reopening the panel shows last known heights.
            return
        }

        subscriptionTasks = blockProviders.flatMap { target, provider in
            [
                fetchInitial(for: target, kind: .best) { try await provider.fetchCurrent() },
                observeHeads(for: target, kind: .best) { provider.subscribeNewHeads() },
                fetchInitial(for: target, kind: .finalized) { try await provider.fetchFinalized() },
                observeHeads(for: target, kind: .finalized) { provider.subscribeFinalizedHeads() }
            ]
        }
    }

    func clear(for target: ChainConnectionTarget) {
        blocks[target] = nil
        finalizedNumbers[target] = nil
        blocksSubject.send(blocks)
    }
}

private extension ChainBlockProvider {
    /// Best heads and finalized heads follow the same fetch-then-stream shape but land in
    /// different state, so the kind selects both the log wording and the recording path.
    enum BlockKind {
        case best
        case finalized

        var title: String {
            switch self {
            case .best:
                "best block"
            case .finalized:
                "finalized block"
            }
        }
    }

    /// Heights would otherwise stay blank for up to a full block time after activation.
    func fetchInitial(
        for target: ChainConnectionTarget,
        kind: BlockKind,
        fetch: @escaping () async throws -> BlockNumber
    ) -> Task<Void, Never> {
        Task { [weak self, logger] in
            do {
                let number = try await withRetry(
                    maxAttempts: Self.initialFetchAttempts,
                    initialDelay: Self.initialReconnectDelay
                ) {
                    try await fetch()
                }

                await self?.record(number, for: target, kind: kind, isFromFetch: true)
            } catch {
                logger.error("Fetch \(kind.title) failed for \(target.chainId): \(error)")
            }
        }
    }

    /// The subscription stream terminates for good on a socket drop, a subscribe-time registry
    /// miss, or a decode failure, so it is reopened with backoff until the task is cancelled.
    func observeHeads(
        for target: ChainConnectionTarget,
        kind: BlockKind,
        headers: @escaping () -> AnyAsyncSequence<Block.Header>
    ) -> Task<Void, Never> {
        Task { [weak self, logger] in
            var delay = Self.initialReconnectDelay

            while !Task.isCancelled {
                var didReceiveHeader = false

                do {
                    for try await header in headers() {
                        didReceiveHeader = true

                        guard let number = BigUInt.fromHexString(header.number) else {
                            logger.warning("Unparsable \(kind.title) number for \(target.chainId): \(header.number)")
                            continue
                        }

                        await self?.record(BlockNumber(number), for: target, kind: kind)
                    }
                } catch {
                    logger.error("Stream of \(kind.title) failed for \(target.chainId): \(error)")
                }

                guard self != nil else {
                    return
                }

                if didReceiveHeader {
                    delay = Self.initialReconnectDelay
                }

                guard await (try? Task.sleep(for: delay)) != nil else {
                    return
                }

                delay = min(delay * 2, Self.maxReconnectDelay)
            }
        }
    }

    func record(
        _ number: BlockNumber,
        for target: ChainConnectionTarget,
        kind: BlockKind,
        isFromFetch: Bool = false
    ) {
        switch kind {
        case .best:
            recordBest(number, for: target, isFromFetch: isFromFetch)
        case .finalized:
            recordFinalized(number, for: target, isFromFetch: isFromFetch)
        }
    }

    func recordBest(
        _ number: BlockNumber,
        for target: ChainConnectionTarget,
        isFromFetch: Bool
    ) {
        // A one-shot fetch may land after the stream already yielded a newer head; drop it.
        guard !isFromFetch || !headsStreamYielded.contains(target) else {
            return
        }

        if !isFromFetch {
            headsStreamYielded.insert(target)
        }

        let currentBlock = blocks[target]
        blocks[target] = ChainBlockInfo(
            number: number,
            receivedAt: Date(),
            finalizedNumber: currentBlock?.finalizedNumber
        )
        blocksSubject.send(blocks)
    }

    func recordFinalized(
        _ number: BlockNumber,
        for target: ChainConnectionTarget,
        isFromFetch: Bool
    ) {
        guard !isFromFetch || !finalizedStreamYielded.contains(target) else {
            return
        }

        let previousNumber = finalizedNumbers[target]

        guard number > previousNumber ?? 0 else {
            return
        }

        if !isFromFetch {
            finalizedStreamYielded.insert(target)
        }

        finalizedNumbers[target] = number

        guard let currentBlock = blocks[target] else {
            return
        }

        blocks[target] = ChainBlockInfo(
            number: currentBlock.number,
            receivedAt: currentBlock.receivedAt,
            finalizedNumber: number
        )
        blocksSubject.send(blocks)
    }
}
