import Foundation
import FoundationExt
import NovaCrypto
import Operation_iOS
import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Takes back the handed-off coins of a payment that no peer claimed, each into a fresh coin of ours.
///
/// Decides only which coins are reclaimable. When the claim window has closed is the caller's call.
public protocol ReclaimCoinsServicing: Sendable {
    /// Schedules a reclaim of every coin of the payment still unclaimed. Returns once they are registered,
    /// not once they settle. Schedules nothing when no coin qualifies, or while a reclaim of `groupId` is
    /// already being scheduled.
    func reclaim(coinKeys: [Data], groupId: CoinageTxGroupId) async throws
}

public actor ReclaimCoinsService: ReclaimCoinsServicing {
    private var inFlight: Set<CoinageTxGroupId> = []

    private let txService: any CoinageTxServicing
    private let databaseFactory: any DatabaseDependencyFactoring
    private let minter: any CoinMinting
    private let snKeyFactory: any SNKeyFactoryProtocol
    private let dateProvider: any DateProviding
    private let logger: SDKLoggerProtocol?

    init(
        txService: any CoinageTxServicing,
        databaseFactory: any DatabaseDependencyFactoring,
        minter: any CoinMinting,
        snKeyFactory: any SNKeyFactoryProtocol,
        dateProvider: any DateProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.txService = txService
        self.databaseFactory = databaseFactory
        self.minter = minter
        self.snKeyFactory = snKeyFactory
        self.dateProvider = dateProvider
        self.logger = logger
    }

    public func reclaim(coinKeys: [Data], groupId: CoinageTxGroupId) async throws {
        guard inFlight.insert(groupId).inserted else {
            logger?.debug("Reclaim already in flight group=\(groupId)")
            return
        }
        defer { inFlight.remove(groupId) }

        let keys = coinKeys.compactMap { try? snKeyFactory.createPublicKey(fromSecret: $0).rawData() }
        let tracked = try await databaseFactory.makeTrackedCoinRepository(publicKeys: keys)
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()

        let reclaimable = tracked.filter(Self.isReclaimable).map(\.coin)
        guard !reclaimable.isEmpty else {
            logger?.debug("Reclaim: nothing to reclaim group=\(groupId)")
            return
        }

        let now = await dateProvider.read()
        var requests: [CoinageScheduledTxRequest] = []
        for coin in reclaimable {
            try await requests.append(request(for: coin, bundleSize: reclaimable.count, now: now))
        }

        logger?.debug("Scheduling \(requests.count) reclaim(s) group=\(groupId)")
        try await txService.scheduleTransactions(requests, groupId: groupId)
    }
}

private extension ReclaimCoinsService {
    /// Committed to the statement store, on chain and never consumed by anything else — the only coins a
    /// reclaim may spend.
    static func isReclaimable(_ tracked: TrackedCoin) -> Bool {
        tracked.coin.isOnchain
            && tracked.coin.handoffMark == .committed
            && tracked.state.consumerStatus == nil
    }

    /// Mints the destination before scheduling, so the row can link its output, as the claim does.
    func request(for coin: Coin, bundleSize: Int, now: Date) async throws -> CoinageScheduledTxRequest {
        let destination = try await minter.mintCoin(
            exponent: coin.exponent,
            provenance: .reclaimed(from: coin, bundleSize: bundleSize)
        )
        let policy = try CoinageSubmissionParams.reclaimPolicy(
            TransferSubmissionParams(
                buildUntil: now.addingTimeInterval(CoinageConstants.reclaimRetryWindow),
                retryFailures: true
            )
        )

        return CoinageScheduledTxRequest(
            policy: policy,
            inputs: [.coin(.reclaimed(coin.derivationIndex, coin.publicKey))],
            outputs: [.coin(destination.derivationIndex, destination.publicKey)]
        )
    }
}
