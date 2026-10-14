import Foundation
import AsyncExtensions

/// Read/write seam for the user-chosen recycling strategy. The implementation lives app-side over
/// `SettingsManager` and is injected into the package, so the evaluator depends only on this protocol.
///
/// `strategyStream()` emits the current value immediately and again on every change, so it can be a
/// `combineLatest` input to the evaluator — changing the strategy re-gates the whole active set on the
/// next tick with no un-triaged intermediate state.
public protocol CoinageRecyclingStrategyProviding: Sendable {
    var strategy: RecyclingStrategyType { get }
    func strategyStream() -> AnyAsyncSequence<RecyclingStrategyType>
    func save(strategy: RecyclingStrategyType)
}
