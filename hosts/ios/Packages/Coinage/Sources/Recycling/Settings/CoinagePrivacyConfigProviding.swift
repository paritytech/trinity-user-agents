import Foundation
import AsyncExtensions

public protocol CoinagePrivacyConfigProviding: Sendable {
    var privacyConfig: CoinagePrivacyConfig { get }
    /// Emits the current value immediately and again on every change.
    func privacyConfigStream() -> AnyAsyncSequence<CoinagePrivacyConfig>
}
