import Foundation
import Operation_iOS
import SubstrateSdk
import StructuredConcurrency
import AsyncExtensions
import AsyncAlgorithms
import BigInt
import SDKLogger

public protocol CoinageBalanceServiceProtocol {
    func start()
    func stop()

    /// The single strategy-aware balance. Amounts are planks; render via ``denominationContext``.
    var balanceStream: AnyAsyncSequence<CoinageBalance> { get }

    /// The balance together with the holdings behind it. A display that shows both must read this
    /// rather than the two separately, so it can never pair one evaluation's totals with another's
    /// holdings.
    var summaryStream: AnyAsyncSequence<CoinageSummary> { get }

    /// The cached denomination context for plank→decimal conversion by display consumers.
    var denominationContext: DenominationBreakdownContext { get }
}

/// Buckets a full snapshot of tracked assets into the strategy-aware three-bucket ``CoinageBalance``,
/// applying the recycling evaluator's coin verdicts and the current strategy's voucher usability.
///
/// Balance runs the pre-classifiers itself on every emission (real-time) and takes only the coin
/// verdicts from the evaluator, so a spend drops the displayed balance immediately rather than up to an
/// interval later. Its own voucher-usability read uses ``BalanceEvaluationMode/immediate`` (peek) ring
/// capacities, so it never blocks on a chain call.
public actor CoinageBalanceService: CoinageBalanceServiceProtocol {
    public nonisolated let denominationContext: DenominationBreakdownContext
    private nonisolated let databaseFactory: any DatabaseDependencyFactoring
    private nonisolated let verdicts: AnyAsyncSequence<RecyclingVerdicts>
    private nonisolated let settings: any CoinageRecyclingStrategyProviding
    private let strategyResolver: any RecyclingStrategyProviding
    private let ringCapacityProvider: any RingCapacityProviding
    private let preClassificator: any CoinageAssetsPreClassificating
    private nonisolated let logger: SDKLoggerProtocol?

    private var balanceSubscriptionTask: Task<Void, Never>?
    private var unlockTimerTask: Task<Void, Never>?

    private var latestCoins: [TrackedCoin] = []
    private var latestVouchers: [TrackedVoucher] = []
    private var latestVerdicts: RecyclingVerdicts = [:]

    /// One subject for both, so an evaluation reaches every subscriber as a single value.
    private nonisolated let summarySubject: AsyncCurrentValueSubject<CoinageSummary>

    init(
        denominationContext: DenominationBreakdownContext,
        databaseFactory: any DatabaseDependencyFactoring,
        verdicts: AnyAsyncSequence<RecyclingVerdicts>,
        settings: any CoinageRecyclingStrategyProviding,
        strategyResolver: any RecyclingStrategyProviding,
        ringCapacityProvider: any RingCapacityProviding,
        preClassificator: any CoinageAssetsPreClassificating,
        logger: SDKLoggerProtocol?
    ) {
        self.denominationContext = denominationContext
        self.databaseFactory = databaseFactory
        self.verdicts = verdicts
        self.settings = settings
        self.strategyResolver = strategyResolver
        self.ringCapacityProvider = ringCapacityProvider
        self.preClassificator = preClassificator
        self.logger = logger

        summarySubject = AsyncCurrentValueSubject<CoinageSummary>(.empty)
    }

    /// Projected off ``summaryStream`` rather than kept in step with it, for the callers that need
    /// only the totals.
    public nonisolated var balanceStream: AnyAsyncSequence<CoinageBalance> {
        summarySubject.map(\.balance).removeDuplicates().eraseToAnyAsyncSequence()
    }

    public nonisolated var summaryStream: AnyAsyncSequence<CoinageSummary> {
        summarySubject.removeDuplicates().eraseToAnyAsyncSequence()
    }

    public nonisolated func start() {
        Task { [weak self] in
            await self?.subscribeToBalances()
        }
    }

    public nonisolated func stop() {
        Task { [weak self] in
            await self?.cancelTasks()
        }
    }
}

private extension CoinageBalanceService {
    func subscribeToBalances() {
        balanceSubscriptionTask?.cancel()
        balanceSubscriptionTask = Task { [weak self] in
            guard let self else { return }
            let coinsStream = databaseFactory.makeTrackedCoinSnapshotStream()
            let vouchersStream = databaseFactory.makeTrackedVoucherSnapshotStream()
            // Combining with `verdicts` withholds the first computation until the evaluator produces one,
            // so balance never flashes zero-available before the first evaluation lands.
            let combined = combineLatest(
                combineLatest(coinsStream, vouchersStream),
                verdicts,
                settings.strategyStream()
            )
            do {
                for try await ((coins, vouchers), verdictMap, _) in combined {
                    await update(coins: coins, vouchers: vouchers, verdicts: verdictMap)
                }
            } catch {
                logger?.error("Balance subscription failed: \(error)")
            }
        }
    }

    func update(coins: [TrackedCoin], vouchers: [TrackedVoucher], verdicts: RecyclingVerdicts) async {
        latestCoins = coins
        latestVouchers = vouchers
        latestVerdicts = verdicts
        await recompute()
    }

    func cancelTasks() {
        balanceSubscriptionTask?.cancel()
        unlockTimerTask?.cancel()
    }

    func recompute() async {
        let now = Date()
        let voucherStrategy = strategyResolver.voucherStrategy(for: settings.strategy)

        let exponents = Set(latestVouchers.map(\.voucher.exponent))
        let capacities = await ringCapacityProvider.peekCapacities(for: exponents)
        let usability = VoucherUsabilityContext(ringCapacities: capacities, now: now)

        let coinBuckets = preClassificator.preClassifyCoins(latestCoins)
        let voucherBuckets = preClassificator.preClassifyVouchers(
            latestVouchers,
            strategy: voucherStrategy,
            context: usability
        )

        // Both derived from this one set of buckets and verdicts, then published as one value:
        // nothing downstream can observe the totals of one evaluation beside another's holdings.
        summarySubject.send(
            CoinageSummary(
                balance: Self.calculateBalance(
                    coinBuckets: coinBuckets,
                    voucherBuckets: voucherBuckets,
                    verdicts: latestVerdicts,
                    canSpendWithConfirmation: voucherStrategy.allowsConfirmedSpend(),
                    context: denominationContext
                ),
                holdings: CoinageHoldings.make(
                    coinBuckets: coinBuckets,
                    voucherBuckets: voucherBuckets,
                    verdicts: latestVerdicts
                )
            )
        )

        scheduleUnlockTimer(for: nextUnlock(among: voucherBuckets.gainingPrivacy, now: now))
    }

    func nextUnlock(among gainingPrivacy: [TrackedVoucher], now: Date) -> Date? {
        let readiness = settings.strategy.params(forcedRecyclingAge: CoinageConstants.recycleAtAge).voucherReadiness
        return gainingPrivacy
            .compactMap { readiness.readyAt(for: $0.voucher) }
            .filter { $0 > now }
            .min()
    }

    func scheduleUnlockTimer(for nextUnlock: Date?) {
        unlockTimerTask?.cancel()
        guard let nextUnlock else { return }

        let interval = nextUnlock.timeIntervalSince(.now)
        guard interval > 0 else { return }

        unlockTimerTask = Task { [weak self] in
            // Add 0.1s buffer so `.now` has passed the target when the task wakes.
            try? await Task.sleep(for: .seconds(interval + 0.1))
            guard !Task.isCancelled, let self else { return }
            await recompute()
        }
    }
}

extension CoinageBalanceService {
    /// Pure bucketing: maps pre-classified assets and coin verdicts into the three-bucket balance.
    /// Extracted (internal, not private) so it can be unit-tested without the actor, streams, or chain
    /// reads.
    static func calculateBalance(
        coinBuckets: CoinBuckets,
        voucherBuckets: VoucherBuckets,
        verdicts: RecyclingVerdicts,
        canSpendWithConfirmation: Bool,
        context: DenominationBreakdownContext
    ) -> CoinageBalance {
        var availableCoins = BigUInt.zero
        var gainingCoins = BigUInt.zero
        var pendingCoins = BigUInt.zero
        for tracked in coinBuckets.minted {
            let amount = context.valueInPlanks(for: tracked.coin.exponent)
            switch verdicts[tracked.coin.derivationIndex] {
            case .allowUse:
                availableCoins += amount
            case .toRecycle:
                gainingCoins += amount
            case .mustRecycle,
                 .none:
                // Chain-forced, or not yet evaluated — both count as pending, never spendable.
                pendingCoins += amount
            }
        }

        let availablePrivate = availableCoins + voucherBuckets.usable.totalPlanks(in: context)
        let gainingAmount = gainingCoins + voucherBuckets.gainingPrivacy.totalPlanks(in: context)
        let pending = pendingCoins
            + coinBuckets.minting.totalPlanks(in: context)
            + voucherBuckets.minting.totalPlanks(in: context)

        return CoinageBalance(
            availablePrivate: availablePrivate,
            gainingPrivacy: .init(amount: gainingAmount, canSpendWithConfirmation: canSpendWithConfirmation),
            pending: pending
        )
    }
}
