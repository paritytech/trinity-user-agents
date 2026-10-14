import Foundation
import SDKLogger

/// Buckets the tracked wallet the way the balance does — usable vouchers are private, the rest of the
/// recycler is gaining privacy — and offers every settled coin for recycling, whatever the strategy
/// holds back: the user has been warned before a low-privacy plan runs.
final class ExternalPaymentAssetClassifier: @unchecked Sendable {
    private let settings: any CoinageRecyclingStrategyProviding
    private let strategyResolver: any RecyclingStrategyProviding
    private let ringCapacityProvider: any RingCapacityProviding
    private let preClassificator: any CoinageAssetsPreClassificating
    private let logger: SDKLoggerProtocol?

    init(
        settings: any CoinageRecyclingStrategyProviding,
        strategyResolver: any RecyclingStrategyProviding,
        ringCapacityProvider: any RingCapacityProviding,
        preClassificator: any CoinageAssetsPreClassificating,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.settings = settings
        self.strategyResolver = strategyResolver
        self.ringCapacityProvider = ringCapacityProvider
        self.preClassificator = preClassificator
        self.logger = logger
    }

    func voucherBuckets(_ vouchers: [TrackedVoucher]) async -> VoucherBuckets {
        let strategy = strategyResolver.voucherStrategy(for: settings.strategy)
        let usability = await usabilityContext(for: vouchers)
        return preClassificator.preClassifyVouchers(vouchers, strategy: strategy, context: usability)
    }

    func recyclableCoins(_ coins: [TrackedCoin]) -> [TrackedCoin] {
        preClassificator.preClassifyCoins(coins).minted
    }
}

private extension ExternalPaymentAssetClassifier {
    func usabilityContext(for vouchers: [TrackedVoucher]) async -> VoucherUsabilityContext {
        let exponents = Set(vouchers.map(\.voucher.exponent))
        let capacities: [Int16: Int]
        do {
            capacities = try await ringCapacityProvider.capacities(for: exponents)
        } catch {
            logger?.warning("Ring capacities unavailable, planning on memoised values: \(error)")
            capacities = await ringCapacityProvider.peekCapacities(for: exponents)
        }
        return VoucherUsabilityContext(ringCapacities: capacities, now: Date())
    }
}
