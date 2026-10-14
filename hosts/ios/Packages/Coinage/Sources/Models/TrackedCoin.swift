import Foundation
import Operation_iOS
import BigInt
import SubstrateSdk

/// A ``Coin`` paired with the durability overlay (``CoinageAssetState``) that determines its
/// balance and selection disposition. Assembled on read; never persisted as-is.
public struct TrackedCoin: Equatable, Sendable {
    public let coin: Coin
    public let state: CoinageAssetState

    public init(coin: Coin, state: CoinageAssetState) {
        self.coin = coin
        self.state = state
    }
}

extension TrackedCoin {
    /// Free of any local claim, present on chain, and young enough to spend. Necessary but not
    /// sufficient for selection — outer layers still apply recycler and locking concerns.
    var isSelectable: Bool {
        state.isFree && coin.isOnchain && coin.isAgeValidToSpend
    }

    /// Not on chain yet, but nothing has proven the entry minting it never ran. Keeps a
    /// freshly-split change coin visible instead of vanishing for a whole mortality window.
    /// Finality is the strongest case for counting it, not the cue to stop: at that point only
    /// the presence the chain reports is behind.
    var isMinting: Bool {
        state.isFree && !coin.isOnchain && state.minterStatus?.canArrive == true
    }

    /// Whether this coin contributes to the displayed balance — spendable, minting (pending), or
    /// awaiting recycling (expiring). Handed-off, consumed, reserved, or vanished coins contribute
    /// nowhere. The single inclusion rule shared by the balance and the coin count/list, so a coin
    /// absent from the balance is absent from both. Mirrors `CoinageBalanceService.calculateBalance`.
    public var isBalanceCounted: Bool {
        isSelectable || isMinting || isAwaitingRecycling()
    }

    /// Whether on-chain presence is still worth tracking. An active coin (neither handed off nor
    /// consumed) is watched so its landing and later disappearance are seen; additionally any coin
    /// still believed on chain stays watched until it is observed to vanish, driving `isOnchain` to
    /// its terminal `false`. A handed-off/consumed coin already off chain is terminal and dropped.
    var isSyncable: Bool {
        (!state.handedOff && !state.isConsumed) || coin.isOnchain
    }

    /// On chain and free, but aged at/past `recycleAtAge` — due for recycling, not spendable.
    func isAwaitingRecycling(for recycleAtAge: Int16 = CoinageConstants.recycleAtAge) -> Bool {
        guard let age = coin.age else { return false }

        return state.isFree && coin.isOnchain && age >= recycleAtAge
    }

    var isRecoverable: Bool {
        state.handedOff && !state.isConsumed
    }
}

extension TrackedCoin: Operation_iOS.Identifiable {
    public var identifier: String {
        coin.identifier
    }
}

public extension [TrackedCoin] {
    /// Summed plank value of these coins in the given denomination context.
    func totalPlanks(in context: DenominationBreakdownContext) -> Balance {
        reduce(BigUInt.zero) { $0 + context.valueInPlanks(for: $1.coin.exponent) }
    }
}

public extension [TrackedVoucher] {
    /// Summed plank value of these vouchers in the given denomination context.
    func totalPlanks(in context: DenominationBreakdownContext) -> Balance {
        reduce(BigUInt.zero) { $0 + context.valueInPlanks(for: $1.voucher.exponent) }
    }
}
