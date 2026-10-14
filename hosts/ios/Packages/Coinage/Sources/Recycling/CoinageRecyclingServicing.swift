import AsyncExtensions
import Foundation

/// Submits recycling for the given coins. The recycling *decision* lives in `CoinRecyclingEvaluator`;
/// this service only performs the submission, one `loadRecyclerWithCoin` extrinsic per coin.
public protocol CoinageRecyclingServicing: Actor {
    /// Recycles the given coins into vouchers. Coins whose ledger state is not free are skipped.
    /// With a `groupId` the batch is registered under that durability group, and a group that already
    /// holds entries is re-joined instead of resubmitted (returns 0).
    /// Returns the number of extrinsics actually submitted — zero when every coin was skipped.
    @discardableResult
    func recycleCoins(_ coins: [Coin], groupId: CoinageTxGroupId?) async throws -> Int

    /// Folds the group's durability entries into a ``RecyclingStatus`` stream; duplicates collapse.
    nonisolated func observeRecycling(groupId: CoinageTxGroupId) -> AnyAsyncSequence<RecyclingStatus>
}
