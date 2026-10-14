import Foundation

/// Coin gate verdicts for a single evaluation, keyed by ``CoinageKeyIndex``. A coin absent from the
/// map has not been evaluated yet and must be treated as pending, never as spendable.
public typealias RecyclingVerdicts = [CoinageKeyIndex: CoinRecyclingState]
