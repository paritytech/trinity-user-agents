import Foundation

/// Per-recycler-group allocation of vouchers and output denominations.
///
/// Each recycler group submits a separate extrinsic, and the pallet validates that
/// each group's total output value equals its total input value.
public struct RecyclerGroupAllocation: Equatable {
    /// The recycler identifier (exponent + index).
    public let recyclerKey: RecyclerKey
    /// Vouchers from this recycler group.
    public let vouchers: [Voucher]
    /// Denominations allocated to recipient from this group's budget.
    public let recipientDenominations: [Denomination]
    /// Denominations allocated to change from this group's budget.
    public let changeDenominations: [Denomination]
}

/// Per-recycler-group allocated coins ready for strategy execution.
///
/// This is the "realized" version of `RecyclerGroupAllocation` where
/// denominations have been converted to actual `Coin` objects with derivation indices.
struct RecyclerGroupCoins: Equatable {
    /// The recycler identifier (exponent + index).
    let recyclerKey: RecyclerKey
    /// Vouchers from this recycler group.
    let vouchers: [Voucher]
    /// Allocated coins for recipient from this group.
    let recipientCoins: [Coin]
    /// Allocated coins for change from this group.
    let changeCoins: [Coin]
}

/// Describes the coin selection result — which strategy to use based on available coins/vouchers.
/// Each case maps to exactly one strategy implementation.
public enum CoinSelectionResult: Equatable {
    /// Strategy 1: Perfect match using existing coins.
    /// - 0 transactions, 0 tokens consumed
    case exactMatch(coins: [Coin])

    /// Strategy 2: Split coin(s) into target and change.
    /// - 1 transaction, 0 tokens consumed
    /// - Origin::Coin (signed with coin keypair)
    /// - `wholeCoins` contribute entirely to target (passed through like exactMatch)
    /// - `overflowCoin` is split to cover remaining amount + generate change
    case split(
        wholeCoins: [Coin],
        overflowCoin: Coin,
        targetDenominations: [Denomination],
        changeDenominations: [Denomination]
    )

    /// Strategy 3: Unload vouchers directly into required denominations.
    /// - 1 transaction & 1 claim token for every voucher group
    /// - Origin::UnloadToken (Ring-VRF proof, no traditional signer)
    /// - coins array may be empty for pure unload scenarios
    /// - perGroupAllocations contains pre-computed per-recycler-group denominations
    ///   ensuring each group's outputs equal its inputs (pallet constraint)
    case unloadIntoCoins(
        coins: [Coin],
        perGroupAllocations: [RecyclerGroupAllocation]
    )
}

extension CoinSelectionResult {
    /// The input coins consumed by this selection.
    var inputCoins: [Coin] {
        switch self {
        case let .exactMatch(coins): coins
        case let .split(wholeCoins, overflowCoin, _, _): wholeCoins + [overflowCoin]
        case let .unloadIntoCoins(coins, _): coins
        }
    }

    /// The input vouchers consumed by this selection.
    var inputVouchers: [Voucher] {
        switch self {
        case .exactMatch,
             .split: []
        case let .unloadIntoCoins(_, perGroupAllocations): perGroupAllocations.flatMap(\.vouchers)
        }
    }
}
