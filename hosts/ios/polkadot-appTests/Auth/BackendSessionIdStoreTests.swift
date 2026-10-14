import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("BackendSessionIdStore")
struct BackendSessionIdStoreTests {
    @Test("Generates UUID on first call")
    func generatesIdOnFirstCall() {
        let settings = InMemorySettingsManager()
        let store = BackendSessionIdStore(settingsManager: settings)

        let sessionId = store.getOrCreateSessionId()

        #expect(!sessionId.isEmpty)
    }

    @Test("Returns same ID on subsequent calls")
    func returnsStableId() {
        let settings = InMemorySettingsManager()
        let store = BackendSessionIdStore(settingsManager: settings)

        let first = store.getOrCreateSessionId()
        let second = store.getOrCreateSessionId()

        #expect(first == second)
    }

    @Test("Persists ID via settings manager")
    func persistsId() {
        let settings = InMemorySettingsManager()
        let store1 = BackendSessionIdStore(settingsManager: settings)

        let original = store1.getOrCreateSessionId()

        let store2 = BackendSessionIdStore(settingsManager: settings)
        let restored = store2.getOrCreateSessionId()

        #expect(original == restored)
    }
}
