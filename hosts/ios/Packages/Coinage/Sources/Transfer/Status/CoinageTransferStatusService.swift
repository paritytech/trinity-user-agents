import AsyncAlgorithms
import AsyncExtensions
import DurableTransactions
import Foundation
import NovaCrypto
import SDKLogger
import SubstrateSdk

public protocol CoinageTransferStatusServicing: Sendable {
    /// The derived payment status of each handed-off coin, keyed by public key. `coinKeys` are the
    /// secret keys carried in the transfer memo; their public keys are derived here. `reclaimGroupId`
    /// names the durable group our reclaims of these coins are scheduled in.
    func subscribeStatuses(
        coinKeys: [Data],
        reclaimGroupId: CoinageTxGroupId
    ) -> AnyAsyncSequence<[PublicKey: CoinageTransferState]>
}

public final class CoinageTransferStatusService: CoinageTransferStatusServicing, @unchecked Sendable {
    private let databaseFactory: any DatabaseDependencyFactoring
    private let chainViewFactory: any PinnedChainViewFactoryProtocol
    private let chainId: ChainId
    private let coinOnChainQuery: any CoinOnChainQuerying
    private let txService: any CoinageTxServicing
    private let snKeyFactory: any SNKeyFactoryProtocol
    private let logger: SDKLoggerProtocol?

    init(
        databaseFactory: any DatabaseDependencyFactoring,
        chainViewFactory: any PinnedChainViewFactoryProtocol,
        chainId: ChainId,
        coinOnChainQuery: any CoinOnChainQuerying,
        txService: any CoinageTxServicing,
        snKeyFactory: any SNKeyFactoryProtocol,
        logger: SDKLoggerProtocol?
    ) {
        self.databaseFactory = databaseFactory
        self.chainViewFactory = chainViewFactory
        self.chainId = chainId
        self.coinOnChainQuery = coinOnChainQuery
        self.txService = txService
        self.snKeyFactory = snKeyFactory
        self.logger = logger
    }

    /// Also re-evaluated on every finalized head: a peer's claim is not our transaction, so nothing local
    /// changes when it finalizes.
    public func subscribeStatuses(
        coinKeys: [Data],
        reclaimGroupId: CoinageTxGroupId
    ) -> AnyAsyncSequence<[PublicKey: CoinageTransferState]> {
        let requested = coinKeys.compactMap { try? snKeyFactory.createPublicKey(fromSecret: $0).rawData() }
        guard !requested.isEmpty else {
            return AsyncStream<[PublicKey: CoinageTransferState]> { $0.finish() }.eraseToAnyAsyncSequence()
        }

        // A filtered subscription to exactly these coins, not the whole set
        // `coinRepository.subscribeCoinsBy(accountIds)`.
        let snapshots = databaseFactory.makeTrackedCoinSnapshotStream(publicKeys: requested)
        // The leading tick lets the first snapshot through before any head arrives.
        let finalizedHeads = chainViewFactory.finalizedHeads(chainId: chainId).prepend(0)
        // Coin state keeps no failed consumer, so a failed reclaim is only visible in its group.
        let failedReclaims = txService.subscribeOperationGroupStatuses(reclaimGroupId)
            .map { $0.failedInputKeys() }
            .removeDuplicates()

        return combineLatest(snapshots, finalizedHeads, failedReclaims)
            .map { [self] tracked, _, failed in await statuses(of: tracked, failedReclaims: failed) }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

// MARK: - Evaluation

private extension CoinageTransferStatusService {
    func statuses(
        of tracked: [TrackedCoin],
        failedReclaims: Set<PublicKey>
    ) async -> [PublicKey: CoinageTransferState] {
        let transferTracked = tracked.filter { Self.reclaimTrackedStatus($0, failedReclaims: failedReclaims) == nil }
        let atFinalized = await presenceAtFinalized(transferTracked)

        var result: [PublicKey: CoinageTransferState] = [:]
        for trackedCoin in tracked {
            result[trackedCoin.coin.publicKey] = CoinageTransferState(
                coin: trackedCoin.coin,
                status: Self.status(trackedCoin, failedReclaims: failedReclaims, atFinalized: atFinalized)
            )
        }
        return result
    }

    /// Whether the finalized chain holds each coin whose mint finalized; absent from the map when
    /// unknown. Only those coins are worth asking about — one has to be minted beyond recall before
    /// its absence can mean the peer took it. A read that cannot be taken leaves them unknown.
    func presenceAtFinalized(_ tracked: [TrackedCoin]) async -> [PublicKey: Bool] {
        let minted = tracked
            .filter { $0.state.minterStatus == .finalizedSuccess }
            .map(\.coin.publicKey)
        guard !minted.isEmpty else { return [:] }

        guard let view = try? await chainViewFactory.pin(chainId: chainId) else { return [:] }
        guard
            let responses = try? await coinOnChainQuery.fetchCoins(for: minted, atBlockHash: view.finalizedHead.hash),
            responses.count == minted.count
        else {
            return [:]
        }

        var presence: [PublicKey: Bool] = [:]
        for (key, response) in zip(minted, responses) {
            presence[key] = response != nil
        }
        return presence
    }
}

// MARK: - Ladder

extension CoinageTransferStatusService {
    static func status(
        _ tracked: TrackedCoin,
        failedReclaims: Set<PublicKey>,
        atFinalized: [PublicKey: Bool]
    ) -> CoinageTransferStatus {
        reclaimTrackedStatus(tracked, failedReclaims: failedReclaims)
            ?? transferTrackedStatus(tracked, atFinalized: atFinalized)
    }

    /// A handed-off coin's only non-failure consumer is our own reclaim, so the consumer decides before any
    /// presence rule does: once a reclaim lands the coin is absent, which the presence rules would misread
    /// as the peer's claim. A failed reclaim of a coin that is gone is left to those rules: the peer won.
    static func reclaimTrackedStatus(_ tracked: TrackedCoin, failedReclaims: Set<PublicKey>) -> CoinageTransferStatus? {
        switch tracked.state.consumerStatus {
        case .finalizedSuccess?: return .reclaimed(finalized: true)
        case .pendingSuccess?: return .reclaimed(finalized: false)
        case let status? where status.isLive: return .reclaiming
        default: break
        }
        return failedReclaims.contains(tracked.coin.publicKey) && tracked.coin.isOnchain ? .reclaimFailed : nil
    }

    static func transferTrackedStatus(_ tracked: TrackedCoin, atFinalized: [PublicKey: Bool]) -> CoinageTransferStatus {
        let coin = tracked.coin
        let state = tracked.state

        // Finalized minter and proven absent at finality: a guaranteed, terminal claim.
        if state.minterStatus == .finalizedSuccess, atFinalized[coin.publicKey] == false {
            return .claimed(finalized: true)
        }

        // Never minted: the key the peer holds controls nothing, and nothing will change that.
        if state.minterStatus == .failure {
            return .failed
        }

        if coin.isOnchain {
            return .awaitingClaim(finalized: atFinalized[coin.publicKey] == true)
        }

        // Seen on chain before, now gone, and its minter has arrived — the peer took it (best head).
        if coin.hasEverBeenOnChain, state.minterStatus?.isArrived == true {
            return .claimed(finalized: false)
        }

        // Absent on best but present at finality: cannot tell "not synced yet" from "gone", so play
        // safe and report awaiting; a later pass marks it claimed once it leaves the finalized block.
        if atFinalized[coin.publicKey] == true {
            return .awaitingClaim(finalized: true)
        }

        return .detecting
    }
}
