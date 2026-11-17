import Foundation
import AsyncExtensions

/// Read/write seam for the user-chosen privacy strategy. The implementation lives app-side over
/// `SettingsManager`. `strategyStream()` emits the current value immediately and again on every change.
public protocol CoinageRecyclingStrategyProviding: Sendable {
    var strategy: RecyclingStrategyType { get }
    func strategyStream() -> AnyAsyncSequence<RecyclingStrategyType>
    func save(strategy: RecyclingStrategyType)
}
