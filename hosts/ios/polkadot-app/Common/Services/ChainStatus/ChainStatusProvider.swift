import Foundation
import AsyncExtensions
import PolkadotUI
import StructuredConcurrency
import FoundationExt
import ChainRegistry

protocol ChainStatusProviding: Actor {
    nonisolated func statusStream() -> AnyAsyncSequence<[ChainConnectionStatusViewModel]>
    func start()
}

/// Per-chain connection status.
/// One shared instance. The subject always holds a row set, so the first render carries a
/// complete set and a host subscribing later sees live state rather than a re-seed.
actor ChainStatusProvider {
    static let statementStoreRowId = "statement-store"

    private static let connectDebounce: Duration = .milliseconds(300)
    private static let deadDwell: TimeInterval = 3
    private static let anchorTimeout: Duration = .seconds(15)

    private let networkStatusService: NetworkStatusProviding
    private let blockProvider: ChainBlockProviding
    private let anchorProvider: ChainLivenessAnchorProviding
    private let appStateStreamFactory: ApplicationStateStreamFactory
    private let statementStoreStatusProvider: StatementStoreStatusProviding
    private let chainRegistry: ChainRegistryProtocol
    private let logger: LoggerProtocol

    private nonisolated let rowsSubject: AsyncCurrentValueSubject<[ChainConnectionStatusViewModel]>

    private var statuses: [ChainConnectionTarget: NetworkStatus]
    private var statementStoreStatus: StatementStoreStatus = .connecting
    private var blocks: [ChainConnectionTarget: ChainBlockInfo] = [:]
    private var liveness: [ChainConnectionTarget: ChainLiveness] = [:]

    private var statusTasks: [Task<Void, Never>] = []
    private var previousIndications: [String: ChainStatusIndication] = [:]
    private var previousLiveness: [String: Double] = [:]
    private var deadSince: [String: Date] = [:]
    private var awaitingReanchor: Set<ChainConnectionTarget> = []
    private var anchorGeneration: [ChainConnectionTarget: Int] = [:]
    private(set) var anchorTasks: [ChainConnectionTarget: Task<Void, Never>] = [:]
    private var tickTask: Task<Void, Never>?
    private var isObserving = false
    private var lastEmittedRows: [ChainConnectionStatusViewModel] = []

    init(
        networkStatusService: NetworkStatusProviding,
        blockProvider: ChainBlockProviding,
        anchorProvider: ChainLivenessAnchorProviding,
        appStateStreamFactory: ApplicationStateStreamFactory,
        statementStoreStatusProvider: StatementStoreStatusProviding,
        chainRegistry: ChainRegistryProtocol,
        logger: LoggerProtocol
    ) {
        self.networkStatusService = networkStatusService
        self.blockProvider = blockProvider
        self.anchorProvider = anchorProvider
        self.appStateStreamFactory = appStateStreamFactory
        self.statementStoreStatusProvider = statementStoreStatusProvider
        self.chainRegistry = chainRegistry
        self.logger = logger

        let seededStatuses = ChainConnectionTarget.allCases
            .reduce(into: [ChainConnectionTarget: NetworkStatus]()) { $0[$1] = .connecting }

        statuses = seededStatuses
        rowsSubject = AsyncCurrentValueSubject(
            Self.makeRows(statuses: seededStatuses, statementStore: .connecting, liveness: [:])
        )
    }

    deinit {
        statusTasks.forEach { $0.cancel() }
        anchorTasks.values.forEach { $0.cancel() }
        tickTask?.cancel()
    }
}

extension ChainStatusProvider: ChainStatusProviding {
    nonisolated func statusStream() -> AnyAsyncSequence<[ChainConnectionStatusViewModel]> {
        rowsSubject.eraseToAnyAsyncSequence()
    }

    func start() {
        guard !isObserving else {
            return
        }

        isObserving = true

        // Sampling runs for the app's lifetime because the top status strip is permanent.
        // A host closing its subscription does not pause sampling.
        let activationTask = Task { [blockProvider] in
            await blockProvider.setActive(true)
        }

        statusTasks = ChainConnectionTarget.allCases.map { target in
            observeStatus(for: target)
        } + [observeBlocks(), observeForeground(), activationTask]

        statementStoreStatusProvider.start()
        statusTasks.append(observeStatementStore(statementStoreStatusProvider))

        tickTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.emitRows()

                try? await Task.sleep(for: .seconds(1))
            }
        }
    }
}

extension ChainStatusProvider {
    func handleStatusUpdate(
        _ status: NetworkStatus,
        for target: ChainConnectionTarget,
        at date: Date = Date()
    ) async {
        let previousStatus = statuses[target]

        guard previousStatus != status else {
            return
        }

        statuses[target] = status

        if status != .connected {
            blocks[target] = nil
            liveness[target]?.clear()
            // Without this a drop-and-reconnect keeps captioning the row with its pre-drop data.
            await blockProvider.clear(for: target)
        } else if previousStatus != .connected, status == .connected {
            applyBlockTime(for: target)
            startAnchor(for: target, at: date)
        }

        emitRows(at: date)
    }

    func handleBlocksUpdate(
        _ updatedBlocks: [ChainConnectionTarget: ChainBlockInfo],
        at date: Date = Date()
    ) {
        guard updatedBlocks != blocks else {
            return
        }

        blocks = updatedBlocks

        for (target, blockInfo) in updatedBlocks {
            liveness[target]?.record(height: blockInfo.number, at: date)
        }

        emitRows(at: date)
    }

    func handleStatementStoreUpdate(_ status: StatementStoreStatus, at date: Date = Date()) {
        guard statementStoreStatus != status else {
            return
        }

        statementStoreStatus = status
        emitRows(at: date)
    }

    func handleForeground(at date: Date = Date()) {
        for target in ChainConnectionTarget.allCases where statuses[target] == .connected {
            awaitingReanchor.insert(target)
            startAnchor(for: target, at: date)
        }

        emitRows(at: date)
    }

    func emitRows(at date: Date = Date()) {
        let rawRows = Self.makeRows(
            statuses: statuses,
            statementStore: statementStoreStatus,
            liveness: liveness
        )
        let indicatedRows = indicateRows(rawRows, at: date)

        guard indicatedRows != lastEmittedRows else { return }

        lastEmittedRows = indicatedRows
        rowsSubject.send(indicatedRows)
    }

    private func indicateRows(
        _ rows: [ChainConnectionStatusViewModel],
        at date: Date
    ) -> [ChainConnectionStatusViewModel] {
        rows.map { row in
            let owner = ChainConnectionTarget.allCases.first { $0.chainId == row.id }
            let targetLiveness = owner
                .flatMap { liveness[$0]?.liveness(at: date) }

            let rawIndication = ChainStatusIndication.resolve(state: row.state, liveness: targetLiveness)

            if
                rawIndication != .dead,
                let owner,
                awaitingReanchor.contains(owner),
                let previous = previousIndications[row.id] {
                return row.withIndication(previous, liveness: previousLiveness[row.id])
            }

            let indication = applyDwell(to: rawIndication, rowId: row.id, at: date)
            previousIndications[row.id] = indication
            if let targetLiveness {
                previousLiveness[row.id] = targetLiveness
            }

            return row.withIndication(indication, liveness: targetLiveness)
        }
    }

    /// Entering dead is held for `deadDwell` so a flap shorter than that never darkens the strip;
    /// leaving dead is immediate. A row that has never been emitted skips the hold, so a cold
    /// launch with no connectivity reads dead at once instead of normal for three seconds.
    private func applyDwell(
        to indication: ChainStatusIndication,
        rowId: String,
        at date: Date
    ) -> ChainStatusIndication {
        guard let previous = previousIndications[rowId] else {
            return indication
        }

        switch (previous, indication) {
        case (.normal, .normal):
            deadSince[rowId] = nil
            return indication
        case (.normal, .outage):
            deadSince[rowId] = nil
            return indication
        case (.normal, .dead):
            let deadAt = deadSince[rowId] ?? date
            deadSince[rowId] = deadAt
            return date.timeIntervalSince(deadAt) < Self.deadDwell ? previous : indication
        case (.outage, .normal):
            deadSince[rowId] = nil
            return indication
        case (.outage, .outage):
            deadSince[rowId] = nil
            return indication
        case (.outage, .dead):
            let deadAt = deadSince[rowId] ?? date
            deadSince[rowId] = deadAt
            return date.timeIntervalSince(deadAt) < Self.deadDwell ? previous : indication
        case (.dead, .normal):
            deadSince[rowId] = nil
            return indication
        case (.dead, .outage):
            deadSince[rowId] = nil
            return indication
        case (.dead, .dead):
            return indication
        }
    }

    static func makeRows(
        statuses: [ChainConnectionTarget: NetworkStatus],
        statementStore: StatementStoreStatus,
        liveness: [ChainConnectionTarget: ChainLiveness]
    ) -> [ChainConnectionStatusViewModel] {
        let targetRows = ChainConnectionTarget.allCases.map { target in
            let state = (statuses[target] ?? .connecting).connectionState

            return ChainConnectionStatusViewModel(
                id: target.chainId,
                title: target.title,
                state: state,
                stateTitle: state.localizedTitle,
                icon: target.statusIcon,
                indication: ChainStatusIndication.resolve(state: state, liveness: nil),
                liveness: nil,
                expectedBlockSeconds: (liveness[target]?.blockPeriod ?? target.fallbackBlockTime).timeInterval
            )
        }

        return targetRows + [makeStatementStoreRow(statementStore)]
    }

    static func makeStatementStoreRow(_ status: StatementStoreStatus) -> ChainConnectionStatusViewModel {
        let state = status.connectionState

        return ChainConnectionStatusViewModel(
            id: Self.statementStoreRowId,
            title: "Statement Store",
            state: state,
            stateTitle: status.localizedTitle,
            icon: .statementStore,
            indication: ChainStatusIndication.resolve(state: state, liveness: nil),
            liveness: nil,
            expectedBlockSeconds: 0,
            showsChainMetrics: false
        )
    }

    /// Rebuilding the window is safe because the caller re-anchors right after.
    private func applyBlockTime(for target: ChainConnectionTarget) {
        let resolved = target.blockTime(from: chainRegistry.getChain(for: target.chainId))

        guard resolved != liveness[target]?.blockPeriod else {
            return
        }

        liveness[target] = ChainLiveness(blockPeriod: resolved)
    }

    private func startAnchor(for target: ChainConnectionTarget, at date: Date) {
        let slotCount = liveness[target]?.slotCount ?? 0
        let generation = (anchorGeneration[target] ?? 0) + 1
        anchorGeneration[target] = generation

        anchorTasks[target] = Task { [weak self, anchorProvider] in
            do {
                // Without a deadline a hung fetch would leave the target in awaitingReanchor forever, freezing its row
                // on the last indication.
                let anchor = try await withTimeout(Self.anchorTimeout) {
                    try await anchorProvider.fetchAnchor(for: target, slotCount: slotCount)
                }
                await self?.finishReanchor(for: target, generation: generation, anchor: anchor, at: date)
            } catch {
                await self?.finishReanchor(for: target, generation: generation, anchor: nil, at: date)
                self?.logger.error("Failed to fetch anchor for \(target.chainId): \(error)")
            }
        }
    }

    private func finishReanchor(
        for target: ChainConnectionTarget,
        generation: Int,
        anchor: ChainLivenessAnchor?,
        at date: Date
    ) {
        guard generation == anchorGeneration[target] else {
            return
        }

        awaitingReanchor.remove(target)

        if let anchor {
            liveness[target]?.apply(anchor, at: date)
        } else {
            liveness[target]?.clear()
        }

        emitRows(at: date)
    }
}

private extension ChainStatusProvider {
    func observeStatus(for target: ChainConnectionTarget) -> Task<Void, Never> {
        Task { [weak self, networkStatusService, logger] in
            let statusStream = networkStatusService
                .statusStream(for: [target.chainId])
                .withDebounce(for: Self.connectDebounce) { $0 == .connected }

            do {
                for try await status in statusStream {
                    await self?.handleStatusUpdate(status, for: target)
                }
            } catch {
                logger.error("Chain status stream failed for \(target.chainId): \(error)")
            }
        }
    }

    func observeStatementStore(_ provider: StatementStoreStatusProviding) -> Task<Void, Never> {
        Task { [weak self, logger] in
            do {
                for try await status in provider.statusStream() {
                    await self?.handleStatementStoreUpdate(status)
                }
            } catch {
                logger.error("Statement store status stream failed: \(error)")
            }
        }
    }

    func observeBlocks() -> Task<Void, Never> {
        Task { [weak self, blockProvider, logger] in
            do {
                for try await blocks in blockProvider.blockStream() {
                    await self?.handleBlocksUpdate(blocks)
                }
            } catch {
                logger.error("Chain block stream failed: \(error)")
            }
        }
    }

    func observeForeground() -> Task<Void, Never> {
        Task { [weak self, appStateStreamFactory] in
            let foregroundStream = appStateStreamFactory.stream(for: .willEnterForeground)
            for await _ in foregroundStream {
                await self?.handleForeground()
            }
        }
    }
}
