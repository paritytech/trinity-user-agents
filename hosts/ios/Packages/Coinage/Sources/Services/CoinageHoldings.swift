import Foundation

/// How the current strategy treats a holding — the same three buckets ``CoinageBalance`` splits the
/// balance into, resolved per holding.
public enum CoinageAvailability: Equatable, Sendable {
    /// Free to use now at no privacy cost.
    case availableNow
    /// Held back to earn privacy. Some strategies release it behind a confirmation.
    case gainingPrivacy
    /// On its way, or past the age the chain still accepts. Not spendable on any terms.
    case pending
}

/// Every holding the user still owns, classified the way the balance classifies it.
///
/// Emitted from the same computation as ``CoinageBalance``, so a display built from these entries and
/// the balance figures shown beside it cannot disagree: summing the entries by ``CoinageAvailability``
/// reproduces the three buckets exactly.
///
/// Coins handed off to a peer are absent — the pre-classifiers admit only free assets.
public struct CoinageHoldings: Equatable, Sendable {
    public struct CoinHolding: Equatable, Sendable {
        public let coin: Coin
        public let availability: CoinageAvailability

        public var isAvailableNow: Bool { availability == .availableNow }

        public init(coin: Coin, availability: CoinageAvailability) {
            self.coin = coin
            self.availability = availability
        }
    }

    public struct VoucherHolding: Equatable, Sendable {
        public let voucher: Voucher
        public let availability: CoinageAvailability

        /// A voucher is available now exactly when the strategy considers it fine to unload
        /// immediately.
        public var isAvailableNow: Bool { availability == .availableNow }

        public init(voucher: Voucher, availability: CoinageAvailability) {
            self.voucher = voucher
            self.availability = availability
        }
    }

    public let coins: [CoinHolding]
    public let vouchers: [VoucherHolding]

    public init(coins: [CoinHolding], vouchers: [VoucherHolding]) {
        self.coins = coins
        self.vouchers = vouchers
    }

    public static let empty = CoinageHoldings(coins: [], vouchers: [])
}

extension CoinageHoldings {
    /// Classifies pre-bucketed assets. Deliberately mirrors
    /// ``CoinageBalanceService/calculateBalance(coinBuckets:voucherBuckets:verdicts:canSpendWithConfirmation:context:)``
    /// case for case, so the two cannot drift: every asset either one counts appears here exactly
    /// once, in the bucket that one puts its value into.
    static func make(
        coinBuckets: CoinBuckets,
        voucherBuckets: VoucherBuckets,
        verdicts: RecyclingVerdicts
    ) -> CoinageHoldings {
        let mintedCoins = coinBuckets.minted.map { tracked in
            let availability: CoinageAvailability =
                switch verdicts[tracked.coin.derivationIndex] {
                case .allowUse: .availableNow
                case .toRecycle: .gainingPrivacy
                // Chain-forced, or not yet evaluated — both count as pending, never spendable.
                case .mustRecycle,
                     .none: .pending
                }

            return CoinHolding(coin: tracked.coin, availability: availability)
        }

        // Still arriving: no verdict has been formed, so never spendable.
        let mintingCoins = coinBuckets.minting.map {
            CoinHolding(coin: $0.coin, availability: .pending)
        }

        let usableVouchers = voucherBuckets.usable.map {
            VoucherHolding(voucher: $0.voucher, availability: .availableNow)
        }
        let waitingVouchers = voucherBuckets.gainingPrivacy.map {
            VoucherHolding(voucher: $0.voucher, availability: .gainingPrivacy)
        }
        let mintingVouchers = voucherBuckets.minting.map {
            VoucherHolding(voucher: $0.voucher, availability: .pending)
        }

        return CoinageHoldings(
            coins: mintedCoins + mintingCoins,
            vouchers: usableVouchers + waitingVouchers + mintingVouchers
        )
    }
}
