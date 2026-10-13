import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("Product runtime settings")
struct ProductRuntimeSettingsTests {
    /// The whole point of the flag's default: a tester who never opens Debug Settings must land on
    /// TrUAPI, so every product surface is exercised against the rust core rather than native.
    /// The test target reads `TRUAPI_RUNTIME_DEFAULT` from the same configuration the app does, so
    /// this compares the declared default against the one the app module compiled.
    @Test("A fresh install runs the TrUAPI runtime")
    func freshInstallRunsTrUAPI() {
        #if TRUAPI_RUNTIME_DEFAULT
            #expect(InMemorySettingsManager().isTrUAPIRuntimeEnabled)
        #else
            #expect(!InMemorySettingsManager().isTrUAPIRuntimeEnabled)
        #endif
    }

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
