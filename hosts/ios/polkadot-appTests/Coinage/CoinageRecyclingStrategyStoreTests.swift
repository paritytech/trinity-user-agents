import Coinage
import Keystore_iOS
import Testing

@testable import polkadot_app

@Suite("Coinage recycling strategy store")
struct CoinageRecyclingStrategyStoreTests {
    @Test("The privacy mode defaults to Balanced when none is stored")
    func defaultsToBalancedWhenNoneStored() {
        let store = CoinageRecyclingStrategyStore(settingsManager: InMemorySettingsManager())

        #expect(store.strategy == .balanced)
    }
}
