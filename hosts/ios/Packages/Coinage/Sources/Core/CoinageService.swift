import struct TrUAPIHost.ProductId
import AsyncAlgorithms
import AsyncExtensions
import BigInt
import Foundation
import SDKLogger
import TrUAPIHost

/// Records whether the user accepted the balance a recovery found.
public protocol CoinageRecoveryAcknowledging: Sendable {
    var isAcknowledged: Bool { get }
    func save(isAcknowledged: Bool)
}

/// ``CoinageServicing`` over the core's coinage layer.
public final class CoinageService: @unchecked Sendable {
    let core: CoinageCore
    let strategySettings: any CoinageRecyclingStrategyProviding
    let recoveryAcknowledgement: any CoinageRecoveryAcknowledging
    let registryConfigured: Bool
    let topUpAcknowledger: (any IncomingPaymentAcknowledging)?
    let logger: SDKLoggerProtocol?

    let deepSearch = AsyncCurrentValueSubject<BackupProgress?>(nil)
    let acknowledged: AsyncCurrentValueSubject<Bool>

    private let lock = NSLock()
    private var privacyTask: Task<Void, Never>?

    /// `registryConfigured` says whether the runtime was given an installation registry, without which
    /// the wallet runs but cannot be recovered from the seed alone.
    public init(
        core: CoinageCore,
        strategySettings: any CoinageRecyclingStrategyProviding,
        recoveryAcknowledgement: any CoinageRecoveryAcknowledging,
        registryConfigured: Bool,
        topUpAcknowledger: (any IncomingPaymentAcknowledging)?,
        logger: SDKLoggerProtocol?
    ) {
        self.core = core
        self.strategySettings = strategySettings
        self.recoveryAcknowledgement = recoveryAcknowledgement
        self.registryConfigured = registryConfigured
        self.topUpAcknowledger = topUpAcknowledger
        self.logger = logger
        acknowledged = AsyncCurrentValueSubject(recoveryAcknowledgement.isAcknowledged)
    }
}

extension CoinageService: CoinageServicing {
    public func units() async throws -> CoinageUnits {
        try await core.units()
    }

    public func start() {
        lock.lock()
        defer { lock.unlock() }

        guard privacyTask == nil else { return }
        privacyTask = Task { [core, strategySettings, logger] in
            do {
                for try await strategy in strategySettings.strategyStream().removeDuplicates() {
                    do {
                        try await core.use { try await $0.setPrivacy(preset: strategy.privacyPreset) }
                    } catch {
                        logger?.error("Failed to apply the coinage privacy setting: \(error)")
                    }
                }
            } catch {
                logger?.error("The privacy setting stream failed: \(error)")
            }
        }
    }

    public func stop() {
        lock.lock()
        defer { lock.unlock() }

        privacyTask?.cancel()
        privacyTask = nil
    }

    public func balanceStream() -> AnyAsyncSequence<CoinageBalance> {
        let core = core
        let strategies = strategySettings.strategyStream()

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let units = try await core.units()
                    for try await (balance, strategy) in combineLatest(core.balances(), strategies) {
                        continuation.yield(balance.coinageBalance(units: units, strategy: strategy))
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    public func summaryStream() -> AnyAsyncSequence<CoinageSummary> {
        let core = core
        let strategies = strategySettings.strategyStream()

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let units = try await core.units()
                    for try await (holdings, strategy) in combineLatest(core.holdings(), strategies) {
                        continuation.yield(
                            CoinageSummary(
                                balance: holdings.balance.coinageBalance(units: units, strategy: strategy),
                                holdings: holdings.coinageHoldings
                            )
                        )
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    public func balance() async throws -> CoinageBalance {
        let units = try await core.units()
        let balance = try await core.use { try await $0.balance() }
        return balance.coinageBalance(units: units, strategy: strategySettings.strategy)
    }

    public func previewTransfer(for amount: BigUInt) async throws -> TransferPreview {
        let current = try await balance()
        guard amount <= current.available else {
            throw TransferPreviewError.insufficientBalance
        }
        return TransferPreview(fullAmount: amount, spendsGainingPrivacy: amount > current.availablePrivate)
    }

    public func prepareTransfer(
        _ preview: TransferPreview,
        exportId: CoinageOperationId,
        retryUntil: Date
    ) async throws -> PreparedTransfer {
        let units = try await core.units()
        let export = try await core.use { coinage in
            try await coinage.export(
                id: exportId.value,
                retry: .until(retryUntil.unixSeconds),
                amount: units.cents(ofPlanks: preview.fullAmount),
                confirmed: preview.spendsGainingPrivacy
            )
        }
        let total = export.coins.reduce(BigUInt(0)) { $0 + units.planks(ofExponent: Int16($1.exponent)) }
        let memo = TransferMemo(entries: export.coins.map(\.secret), totalValue: total)

        return PreparedTransfer(memo: memo, exportId: exportId, core: core)
    }

    public func subscribePaymentStates(exportId: CoinageOperationId) -> AnyAsyncSequence<[CoinagePaymentState]> {
        let core = core

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let units = try await core.units()
                    for try await coins in core.exportStatuses(of: exportId) {
                        continuation.yield(coins.map { $0.paymentState(units: units) })
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        .eraseToAnyAsyncSequence()
    }

    public func claim(
        coinKeys: [Data],
        operationId: CoinageOperationId,
        retryUntil: Date
    ) -> AnyAsyncSequence<CoinageTransferDetection> {
        followOperation(operationId) { coinage in
            try await coinage.import(id: operationId.value, retry: .until(retryUntil.unixSeconds), coins: coinKeys)
        }
        .eraseToAnyAsyncSequence()
    }
}

extension CoinageService {
    /// Starts the operation `id` with `start`, then follows it until it ends.
    func followOperation(
        _ id: CoinageOperationId,
        start: @escaping @Sendable (NativeCoinage) async throws -> Started
    ) -> AsyncThrowingStream<CoinageTransferDetection, Error> {
        let core = core

        return AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    _ = try await core.use(start)
                    for try await detection in Self.follow(id, core: core) {
                        continuation.yield(detection)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// Follows the operation `id`, already started, until its terminal status.
    static func follow(_ id: CoinageOperationId, core: CoinageCore) -> AsyncThrowingStream<CoinageTransferDetection, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let units = try await core.units()
                    for try await item in core.statuses(of: id) {
                        continuation.yield(item.transferDetection(units: units))
                        if item.status.isTerminal { break }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
