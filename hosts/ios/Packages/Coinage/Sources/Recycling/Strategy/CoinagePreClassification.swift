import Foundation
import SubstrateSdk
import BigInt

/// Pure partition of the free coin set. The only coins eligible for gating are ``minted``; nothing
/// outside it is ever passed to `evaluate`, which is the first guard against gating an unknown-age coin.
public struct CoinBuckets: Equatable {
    /// On chain with a known age — settled and gate-eligible.
    public let minted: [TrackedCoin]
    /// Still arriving: not on chain yet, or on chain but age not yet observed.
    public let minting: [TrackedCoin]

    /// Everything free — the value denominator.
    public var all: [TrackedCoin] { minted + minting }
}

/// Pure partition of the free voucher set under a given strategy.
public struct VoucherBuckets: Equatable {
    /// In the recycler and usable now under the current strategy.
    public let usable: [TrackedVoucher]
    /// In the recycler but not yet usable — gaining privacy.
    public let gainingPrivacy: [TrackedVoucher]
    /// Onboarding or still minting.
    public let minting: [TrackedVoucher]

    /// Everything free — the value denominator.
    public var all: [TrackedVoucher] { usable + gainingPrivacy + minting }
}

public protocol CoinageAssetsPreClassificating {
    func preClassifyCoins(_ coins: [TrackedCoin]) -> CoinBuckets

    func preClassifyVouchers(
        _ vouchers: [TrackedVoucher],
        strategy: CoinRecyclingStrategyProtocol,
        context: VoucherUsabilityContext
    ) -> VoucherBuckets
}

public class CoinageAssetPreClassificator: CoinageAssetsPreClassificating {
    /// Partitions free coins into settled (gate-eligible) and still-arriving. An on-chain coin whose age
    /// is not yet observed lands in ``CoinBuckets/minting``, never ``CoinBuckets/minted``.
    public func preClassifyCoins(_ coins: [TrackedCoin]) -> CoinBuckets {
        var minted: [TrackedCoin] = []
        var minting: [TrackedCoin] = []

        for tracked in coins where tracked.state.isFree {
            if tracked.coin.isOnchain, tracked.coin.hasEverBeenOnChain {
                minted.append(tracked)
            } else if tracked.coin.isOnchain || tracked.isMinting {
                // On chain but age not yet observed, or not on chain yet but minting — still arriving.
                minting.append(tracked)
            }
        }

        return CoinBuckets(minted: minted, minting: minting)
    }

    /// Partitions free vouchers into usable / gaining-privacy / minting. Usability is decided by the
    /// strategy, which is why the strategy is a parameter — balance calls this with a bare
    /// ``ParametricRecyclingStrategy`` (no quota read needed for the voucher arm).
    public func preClassifyVouchers(
        _ vouchers: [TrackedVoucher],
        strategy: CoinRecyclingStrategyProtocol,
        context: VoucherUsabilityContext
    ) -> VoucherBuckets {
        var usable: [TrackedVoucher] = []
        var gainingPrivacy: [TrackedVoucher] = []
        var minting: [TrackedVoucher] = []

        for tracked in vouchers where tracked.state.isFree {
            if tracked.voucher.isInRecycler {
                if strategy.isVoucherUsable(tracked.voucher, context: context) {
                    usable.append(tracked)
                } else {
                    gainingPrivacy.append(tracked)
                }
            } else if tracked.isOnboarding || tracked.isMinting {
                minting.append(tracked)
            }
        }

        return VoucherBuckets(usable: usable, gainingPrivacy: gainingPrivacy, minting: minting)
    }
}
