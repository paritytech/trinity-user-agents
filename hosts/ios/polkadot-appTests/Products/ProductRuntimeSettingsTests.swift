import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("Product runtime settings")
struct ProductRuntimeSettingsTests {
    @Test("An explicit switch to native is respected")
    func storedNativeChoiceWins() {
        let settings = InMemorySettingsManager()
        settings.set(value: false, for: .truApiRuntimeEnabled)

        #expect(!settings.isTrUAPIRuntimeEnabled)
    }

    @Test("An explicit switch to TrUAPI is respected")
    func storedTrUAPIChoiceWins() {
        let settings = InMemorySettingsManager()
        settings.set(value: true, for: .truApiRuntimeEnabled)

        #expect(settings.isTrUAPIRuntimeEnabled)
    }

    /// `value(for:)` is shared by every boolean setting in the app, so it cannot carry this key's
    /// default. Reading the key through it is the trap this accessor exists to close.
    @Test("The shared boolean helper still defaults to off")
    func sharedHelperDoesNotCarryTheDefault() {
        #expect(!InMemorySettingsManager().value(for: .truApiRuntimeEnabled))
    }
}
