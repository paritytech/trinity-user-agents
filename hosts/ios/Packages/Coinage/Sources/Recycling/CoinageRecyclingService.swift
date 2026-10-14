import Foundation
import ExtrinsicService
import KeyDerivation
import Keystore_iOS
import Operation_iOS
import SDKLogger
import SubstrateSdk
import StructuredConcurrency
import BackgroundExecution
import AsyncExtensions

/// Persisted recycling intent handed from `prepareRecycle` to the submission step.
private struct PreparedRecycle {
    let voucher: Voucher
    let origin: any ExtrinsicOriginDefining
    let builder: ExtrinsicBuilderClosure
}

/// Submits coin recycling. The decision of *which* coins to recycle lives in `CoinRecyclingEvaluator`;
/// this service only submits, fire-and-forget, one `loadRecyclerWithCoin` extrinsic per coin. The
/// durability layer resolves each outcome; a coin whose extrinsic never lands is released by the
/// recovery pass at mortality, so there is nothing to roll back here.
actor CoinageRecyclingService {
    private let voucherMinter: any VoucherMinting
    private let coinKeypairFactory: any CoinKeyDeriving
    private let voucherKeypairFactory: any VoucherKeyDeriving
    private let txService: any CoinageTxServicing
    private let voucherService: VoucherServiceProtocol
    private let originFactory: OriginCreating
    private let backgroundExecutor: any BackgroundExecuting
    private let logger: SDKLoggerProtocol

    init(
        voucherMinter: any VoucherMinting,
        coinKeypairFactory: any CoinKeyDeriving,
        voucherKeypairFactory: any VoucherKeyDeriving,
        txService: any CoinageTxServicing,
        voucherService: VoucherServiceProtocol,
        originFactory: OriginCreating,
        backgroundExecutor: any BackgroundExecuting,
        logger: SDKLoggerProtocol
    ) {
        self.voucherMinter = voucherMinter
        self.coinKeypairFactory = coinKeypairFactory
        self.voucherKeypairFactory = voucherKeypairFactory
        self.txService = txService
        self.voucherService = voucherService
        self.originFactory = originFactory
        self.backgroundExecutor = backgroundExecutor
        self.logger = logger
    }
}

// MARK: - CoinageRecyclingServicing

extension CoinageRecyclingService: CoinageRecyclingServicing {
    /// Prepares every recycle up front, then submits them as one atomic batch: the durability write —
    /// and so the evaluator's re-trigger off the coin snapshot — happens once, not once per coin. The
    /// background-task assertion lets a fold mid-submission still finish registering the batch.
    @discardableResult
    func recycleCoins(_ coins: [Coin], groupId: CoinageTxGroupId?) async throws -> Int {
        if let groupId, try await !txService.getOperationGroupStatuses(groupId).isEmpty {
            logger.debug("Recycling group \(groupId) already registered, re-joining")
            return 0
        }

        return try await backgroundExecutor.execute { [self] in
            try await submitRecycle(coins, groupId: groupId)
        }
    }

    nonisolated func observeRecycling(groupId: CoinageTxGroupId) -> AnyAsyncSequence<RecyclingStatus> {
        let voucherService = voucherService

        return txService.subscribeOperationGroupStatuses(groupId)
            .map { entries -> RecyclingStatus in
                try await RecyclingStatusFolder.fold(entries: entries, voucherService: voucherService)
            }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}

/// Pure folding of a recycling group's entries into a ``RecyclingStatus``: any failure is `incomplete`,
/// anything still unincluded is `pending`, otherwise `allRecycled`.
enum RecyclingStatusFolder {
    static func fold(
        entries: [CoinageTxEntry],
        voucherService: VoucherServiceProtocol
    ) async throws -> RecyclingStatus {
        guard !entries.contains(where: { $0.status == .failure }) else {
            return .incomplete
        }

        // Best-block inclusion (`pendingSuccess`) already counts as recycled; `finalized` says whether
        // every entry has also finalized.
        guard !entries.isEmpty, entries.allSatisfy(\.status.isArrived) else {
            return .pending
        }

        let mintedIndices = Set(entries.flatMap(\.outputs).compactMap { output -> CoinageKeyIndex? in
            guard case let .recyclerVoucher(index, _) = output else { return nil }
            return index
        })
        let vouchers = try await voucherService.fetchTracked(derivationIndices: mintedIndices)

        // Arrival is read at the best head while the recycler location lags behind it, and an unload
        // needs the latter.
        guard vouchers.count == mintedIndices.count, vouchers.allSatisfy(\.voucher.isInRecycler) else {
            return .pending
        }

        return .allRecycled(
            vouchers: vouchers,
            finalized: entries.allSatisfy { $0.status == .finalizedSuccess }
        )
    }
}

// MARK: - Private

private extension CoinageRecyclingService {
    func submitRecycle(_ coins: [Coin], groupId: CoinageTxGroupId?) async throws -> Int {
        var requests: [CoinageTxRequest] = []

        for coin in coins {
            do {
                let prepared = try await prepareRecycle(coin)
                requests.append(
                    CoinageTxRequest(
                        inputs: [.coin(.own(coin.derivationIndex, coin.publicKey))],
                        outputs: [.recyclerVoucher(prepared.voucher.derivationIndex, prepared.voucher.publicKey)],
                        builder: prepared.builder,
                        origin: prepared.origin
                    )
                )
            } catch {
                logger.error("Coin recycling failed for \(coin.derivationIndex): \(error)")
            }
        }

        guard !requests.isEmpty else { return 0 }

        try await txService.submitTransactions(requests, groupId: groupId)
        return requests.count
    }

    /// Locks the coin, allocates the voucher, and persists the voucher (`.pendingOnboarding`)
    /// before submission so a crash mid-flight is recoverable. Throws before anything is broadcast.
    func prepareRecycle(_ coin: Coin) async throws -> PreparedRecycle {
        // The coin is reserved by its `load_recycler` durability entry at submission — a live
        // input derives to `.pendingTransfer` — so no separate pre-lock is written here.
        let voucher = try await voucherMinter.mintVoucher(exponent: coin.exponent)

        let memberKey = voucher.publicKey
        let keyManager = try voucherKeypairFactory.createKeyManager(for: voucher)
        let coinPublicKey = coin.publicKey
        let proof = try keyManager.sign(coinPublicKey)

        let call = CoinagePallet.Calls.LoadRecyclerWithCoin(
            memberKey: memberKey,
            proofOfOwnership: proof
        )
        let coinPrivateKey = try coinKeypairFactory.derivePrivateKey(for: coin)
        let coinWallet = DynamicDerivedWallet(secretKeyProvider: { coinPrivateKey })
        let origin = try originFactory.createAsCoinOrigin(for: coinWallet)
        let builder: ExtrinsicBuilderClosure = { try $0.adding(call: call.callAsFunction()) }

        // The voucher is already persisted by the allocator, so a crash mid-flight is recoverable.
        return PreparedRecycle(voucher: voucher, origin: origin, builder: builder)
    }
}
