import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import StructuredConcurrency
import SDKLogger
import AsyncExtensions
import CommonService
import KeyDerivation

/// A service that monitors local coins and synchronizes their on-chain state.
public final class CoinStateSyncService: BaseSyncService {
    private let coinService: CoinServiceProtocol
    private let databaseFactory: any DatabaseDependencyFactoring
    private let connection: JSONRPCEngine
    private let runtimeService: RuntimeCodingServiceProtocol
    private let instanceId: CoinageInstanceId

    private var localCoinsMonitoringTask: Task<Void, Error>?
    private var syncTask: Task<Void, Error>?

    public init(
        coinService: CoinServiceProtocol,
        databaseFactory: any DatabaseDependencyFactoring,
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        instanceId: CoinageInstanceId,
        logger: any SDKLoggerProtocol
    ) {
        self.coinService = coinService
        self.databaseFactory = databaseFactory
        self.connection = connection
        self.runtimeService = runtimeService
        self.instanceId = instanceId
        super.init(logger: logger)
    }

    deinit {
        stopSyncUp()
    }

    /// Begins monitoring the local database for coins that require on-chain status updates.
    /// It filters for coins that are not spent.
    override public func performSyncUp() {
        localCoinsMonitoringTask = Task { [weak self] in
            guard let self else { return }

            let stream = databaseFactory.makeTrackedCoinSnapshotStream()
                .map { (tracked: [TrackedCoin]) -> [Coin] in
                    tracked.map(\.coin)
                }
                .removeDuplicates()

            for try await coins in stream {
                guard !coins.isEmpty else {
                    logger.debug("Coin sync stopped")
                    syncTask?.cancel()
                    continue
                }
                try Task.checkCancellation()

                do {
                    logger.debug("Coin sync started")
                    try await performSync(coins)
                } catch {
                    logger.error("Coin sync failed during monitoring: \(error)")
                }
            }
        }
    }

    override public func stopSyncUp() {
        localCoinsMonitoringTask?.cancel()
        syncTask?.cancel()
    }
}

extension CoinStateSyncService {
    /// Orchestrates the on-chain subscription for a set of coins.
    /// Cancels existing subscriptions and creates a new batch request.
    private func performSync(_ coins: [Coin]) async throws {
        syncTask?.cancel()

        let requests: [BatchStorageSubscriptionRequest] = coins.map { coin in
            let publicKey = coin.publicKey
            let mappingKey = publicKey.toHex()
            let storagePath = CoinagePallet.Storage.coinsByOwner
            let innerRequest = MapSubscriptionRequest(
                storagePath: storagePath(),
                localKey: "",
                keyParamClosure: { BytesCodable(wrappedValue: publicKey) }
            )
            return BatchStorageSubscriptionRequest(innerRequest: innerRequest, mappingKey: mappingKey)
        }

        guard !requests.isEmpty else { return }

        syncTask = Task { [weak self] in
            guard let self else { return }

            let stream: AnyAsyncSequence<CoinSyncResult> = CallbackBatchStorageSubscription
                .asyncStream(
                    requests: requests,
                    connection: connection,
                    runtimeService: runtimeService,
                    logger: logger
                )

            for try await result in stream {
                try Task.checkCancellation()
                try await handleSubscriptionUpdate(result)
            }
        }
    }

    private func handleSubscriptionUpdate(_ result: CoinSyncResult) async throws {
        let availableCoins = try await coinService.fetchAllTrackedCoins()
        guard !availableCoins.isEmpty else { return }

        var coinMap: [String: Coin] = [:]
        for tracked in availableCoins where !tracked.state.isConsumed {
            let coin = tracked.coin
            coinMap[coin.publicKey.toHex()] = coin
        }

        var updates: [CoinPresenceUpdate] = []

        for (mappingKey, read) in result.reads(in: instanceId) {
            guard let coin = coinMap[mappingKey],
                  let update = presenceUpdate(for: coin, read: read, mappingKey: mappingKey)
            else { continue }

            updates.append(update)
        }

        guard !updates.isEmpty else { return }
        // A dedicated write-only mapper touches only age + isOnchain, so a concurrent change to any
        // other coin field is not clobbered by this presence write.
        try await databaseFactory.makeCoinPresenceRepository()
            .saveOperation({ updates }, { [] })
            .asyncExecute()
        logger.debug("Updated \(updates.count) coins via sync subscription")
    }

    /// What sync records for a tracked coin from the chain's read of its key, or `nil` when nothing changed.
    private func presenceUpdate(for coin: Coin, read: OnChainCoinRead, mappingKey: String) -> CoinPresenceUpdate? {
        if case let .foreign(foreignInstanceId) = read {
            logger.warning("TrackingCoin: \(mappingKey) holds a foreign coin of instance \(foreignInstanceId)")
        }

        guard let onChainCoin = read.coin else {
            // Seen before and now absent -> a peer claimed it; derived as spent. Age is kept.
            guard coin.age != nil, coin.isOnchain else { return nil }
            return CoinPresenceUpdate(derivationIndex: coin.derivationIndex, age: coin.age, isOnchain: false)
        }

        // Present on chain -> record presence and sync age.
        if Int16(onChainCoin.value) != coin.exponent {
            logger.error(
                "TrackingCoin: \(mappingKey) exponent \(coin.exponent) " +
                    "doesn't match on chain exponent \(onChainCoin.value)"
            )
        }
        let onChainAge = onChainCoin.age
        guard coin.age != onChainAge || !coin.isOnchain else { return nil }
        return CoinPresenceUpdate(derivationIndex: coin.derivationIndex, age: onChainAge, isOnchain: true)
    }
}
