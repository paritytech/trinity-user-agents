import Foundation
import AsyncExtensions
import AsyncAlgorithms
import Operation_iOS
import SubstrateSdk
import SDKLogger

/// Derives coin recycling verdicts on a throttled cadence and triggers recycling for gated coins.
///
/// The verdict depends on the current balance ratio and on the ages of every other active coin, both
/// of which move, so it is computed, never persisted. Balance consumes ``verdicts`` (deduplicated);
/// the recycle trigger fires from the evaluation body, not from a verdict observer, so an unchanged
/// verdict after a failed recycle still retries.
public actor CoinRecyclingEvaluator {
    private nonisolated let databaseFactory: any DatabaseDependencyFactoring
    private nonisolated let settings: any CoinageRecyclingStrategyProviding
    private let strategyProvider: any RecyclingStrategyProviding
    private let ringCapacityProvider: any RingCapacityProviding
    private let preClassificator: any CoinageAssetsPreClassificating
    private let recyclingService: any CoinageRecyclingServicing
    private nonisolated let denominationContext: DenominationBreakdownContext
    private nonisolated let logger: SDKLoggerProtocol?

    /// Nil until the first evaluation completes — balance must not emit before it, or it reads as zero.
    private nonisolated let verdictsSubject = AsyncCurrentValueSubject<RecyclingVerdicts?>(nil)

    private var inputTask: Task<Void, Never>?

    private static let evaluationInterval: Duration = .seconds(5)

    init(
        databaseFactory: any DatabaseDependencyFactoring,
        settings: any CoinageRecyclingStrategyProviding,
        strategyProvider: any RecyclingStrategyProviding,
        ringCapacityProvider: any RingCapacityProviding,
        preClassificator: any CoinageAssetsPreClassificating,
        recyclingService: any CoinageRecyclingServicing,
        denominationContext: DenominationBreakdownContext,
        logger: SDKLoggerProtocol?
    ) {
        self.databaseFactory = databaseFactory
        self.settings = settings
        self.strategyProvider = strategyProvider
        self.ringCapacityProvider = ringCapacityProvider
        self.preClassificator = preClassificator
        self.recyclingService = recyclingService
        self.denominationContext = denominationContext
        self.logger = logger
    }

    /// Verdicts stream — deduplicated and withheld until the first evaluation lands.
    public nonisolated var verdicts: AnyAsyncSequence<RecyclingVerdicts> {
        verdictsSubject.compactMap { $0 }.eraseToAnyAsyncSequence()
    }

    /// The latest verdicts, or nil before the first evaluation. Read synchronously by selection so a
    /// transfer draws only on coins the current strategy leaves spendable.
    public nonisolated func currentVerdicts() -> RecyclingVerdicts? {
        verdictsSubject.value
    }

    public nonisolated func start() {
        Task { [weak self] in await self?.run() }
    }

    public nonisolated func stop() {
        Task { [weak self] in await self?.cancelTasks() }
    }
}

private extension CoinRecyclingEvaluator {
    struct Assets {
        let coins: [TrackedCoin]
        let vouchers: [TrackedVoucher]
    }

    struct Input {
        let assets: Assets
        let strategy: RecyclingStrategyType
    }

    func run() {
        let coinsStream = databaseFactory.makeTrackedCoinSnapshotStream()
        let vouchersStream = databaseFactory.makeTrackedVoucherSnapshotStream()

        let leadingTick: AsyncSyncSequence<[Void]> = [()].async
        let tick = chain(
            leadingTick,
            AsyncTimerSequence(interval: Self.evaluationInterval, clock: ContinuousClock()).map { _ in () }
        )

        inputTask = Task { [settings, weak self] in
            let assets = combineLatest(coinsStream, vouchersStream).map { coins, vouchers in
                Assets(coins: coins, vouchers: vouchers)
            }

            let throttledAssets = combineLatest(assets, tick)._throttle(for: Self.evaluationInterval, latest: true)
            let inputStream = combineLatest(throttledAssets, settings.strategyStream()).map { assets, strategy in
                Input(assets: assets.0, strategy: strategy)
            }

            do {
                for try await input in inputStream {
                    await self?.evaluateAndRecycle(for: input)
                }
            } catch {
                self?.logger?.error("Recycling evaluator asset stream failed: \(error)")
            }
        }
    }

    func cancelTasks() {
        inputTask?.cancel()
    }

    func evaluateAndRecycle(for input: Input) async {
        let now = Date()

        // Fast pass first, but only until a verdict exists: it applies just the chain age-ceiling (no
        // quota or capacity read) so balance renders without waiting on the chain. Recycling never runs
        // off it — recycling is a chain write, so starting it here buys nothing over the complete pass.
        if verdictsSubject.value == nil,
           let immediate = try? await evaluate(input: input, now: now, mode: .immediate) {
            verdictsSubject.send(immediate)
        }

        do {
            let complete = try await evaluate(input: input, now: now, mode: .complete)
            verdictsSubject.send(complete)
            await recycleGated(complete, coins: input.assets.coins)
        } catch {
            logger?.error("Recycling evaluation failed: \(error)")
        }
    }

    func evaluate(input: Input, now: Date, mode: BalanceEvaluationMode) async throws -> RecyclingVerdicts {
        let vouchers = input.assets.vouchers
        let coins = input.assets.coins
        let type = input.strategy

        let voucherExponents = Set(vouchers.map(\.voucher.exponent))
        let capacities =
            switch mode {
            case .complete: try await ringCapacityProvider.capacities(for: voucherExponents)
            case .immediate: await ringCapacityProvider.peekCapacities(for: voucherExponents)
            }
        let usabilityContext = VoucherUsabilityContext(ringCapacities: capacities, now: now)

        let coinBuckets = preClassificator.preClassifyCoins(coins)
        let voucherBuckets = preClassificator.preClassifyVouchers(
            vouchers,
            strategy: strategyProvider.voucherStrategy(for: type),
            context: usabilityContext
        )

        let snapshot = RecyclingSnapshot(
            total: coinBuckets.all.totalPlanks(in: denominationContext)
                + voucherBuckets.all.totalPlanks(in: denominationContext),
            unavailable: coinBuckets.minting.totalPlanks(in: denominationContext)
                + voucherBuckets.minting.totalPlanks(in: denominationContext)
                + voucherBuckets.gainingPrivacy.totalPlanks(in: denominationContext)
        )

        let coinStrategy = try await strategyProvider.coinStrategy(for: type, mode: mode)
        return coinStrategy.evaluate(
            coins: coinBuckets.minted.map(\.coin),
            snapshot: snapshot,
            context: denominationContext
        )
    }

    func recycleGated(_ verdicts: RecyclingVerdicts, coins: [TrackedCoin]) async {
        let gated = Set(verdicts.filter(\.value.triggersRecycling).keys)
        guard !gated.isEmpty else { return }

        // `recycleCoins` skips coins whose ledger state is not free, so the 5s cadence is idempotent.
        let coinsToRecycle = coins.filter { gated.contains($0.coin.derivationIndex) }.map(\.coin)
        do {
            try await recyclingService.recycleCoins(coinsToRecycle, groupId: nil)
        } catch {
            logger?.error("Recycle trigger failed: \(error)")
        }
    }
}
