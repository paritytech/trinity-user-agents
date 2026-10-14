import AsyncAlgorithms
import AsyncExtensions
import DurableTransactions
import Foundation
import NovaCrypto
import SDKLogger
import SubstrateSdk

public protocol CoinageTransferStatusServicing: Sendable {
    /// The derived payment status of each handed-off coin, keyed by public key. `coinKeys` are the
    /// secret keys carried in the transfer memo; their public keys are derived here.
    func subscribeStatuses(coinKeys: [Data]) -> AnyAsyncSequence<[PublicKey: CoinageTransferState]>
}

public final class CoinageTransferStatusService: CoinageTransferStatusServicing, @unchecked Sendable {
    private let databaseFactory: any DatabaseDependencyFactoring
    private let chainViewFactory: any PinnedChainViewFactoryProtocol
    private let chainId: ChainId
    private let coinOnChainQuery: any CoinOnChainQuerying
    private let snKeyFactory: any SNKeyFactoryProtocol
    private let logger: SDKLoggerProtocol?

    init(
        databaseFactory: any DatabaseDependencyFactoring,
        chainViewFactory: any PinnedChainViewFactoryProtocol,
        chainId: ChainId,
        coinOnChainQuery: any CoinOnChainQuerying,
        snKeyFactory: any SNKeyFactoryProtocol,
        logger: SDKLoggerProtocol?
    ) {
        self.databaseFactory = databaseFactory
        self.chainViewFactory = chainViewFactory
        self.chainId = chainId
        self.coinOnChainQuery = coinOnChainQuery
        self.snKeyFactory = snKeyFactory
        self.logger = logger
    }

    /// Also re-evaluated on every finalized head: a peer's claim is not our transaction, so nothing local
    /// changes when it finalizes.
    public func subscribeStatuses(coinKeys: [Data]) -> AnyAsyncSequence<[PublicKey: CoinageTransferState]> {
        let requested = coinKeys.compactMap { try? snKeyFactory.createPublicKey(fromSecret: $0).rawData() }
        guard !requested.isEmpty else {
            return AsyncStream<[PublicKey: CoinageTransferState]> { $0.finish() }.eraseToAnyAsyncSequence()
        }

        // A filtered subscription to exactly these coins, not the whole set
        // `coinRepository.subscribeCoinsBy(accountIds)`.
        let snapshots = databaseFactory.makeTrackedCoinSnapshotStream(publicKeys: requested)
        // The leading tick lets the first snapshot through before any head arrives.
        let finalizedHeads = chainViewFactory.finalizedHeads(chainId: chainId).prepend(0)

        return combineLatest(snapshots, finalizedHeads)
            .map { [self] tracked, _ in await statuses(of: tracked) }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

// MARK: - Evaluation

private extension CoinageTransferStatusService {
    func statuses(of tracked: [TrackedCoin]) async -> [PublicKey: CoinageTransferState] {
        let atFinalized = await presenceAtFinalized(tracked)

        var result: [PublicKey: CoinageTransferState] = [:]
        for trackedCoin in tracked {
            result[trackedCoin.coin.publicKey] = CoinageTransferState(
                coin: trackedCoin.coin,
                status: Self.transferStatus(trackedCoin, atFinalized: atFinalized)
            )
        }
        return result
    }
}

// MARK: - Ladder

extension CoinageTransferStatusService {
    static func transferStatus(_ tracked: TrackedCoin, atFinalized: [PublicKey: Bool]) -> CoinageTransferStatus {
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
            return .awaitingClaim
        }

        // Seen on chain before, now gone, and its minter has arrived — the peer took it (best head).
        if coin.hasEverBeenOnChain, state.minterStatus?.isArrived == true {
            return .claimed(finalized: false)
        }

        // Absent on best but present at finality: cannot tell "not synced yet" from "gone", so play
        // safe and report awaiting; a later pass marks it claimed once it leaves the finalized block.
        if atFinalized[coin.publicKey] == true {
            return .awaitingClaim
        }

        return .detecting
    }

    /// Whether the finalized chain holds each coin whose mint finalized; absent from the map when
    /// unknown. Only those coins are worth asking about — one has to be minted beyond recall before
    /// its absence can mean the peer took it. A read that cannot be taken leaves them unknown.
    private func presenceAtFinalized(_ tracked: [TrackedCoin]) async -> [PublicKey: Bool] {
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
