import Foundation
import Keystore_iOS
import Testing

@testable import polkadot_app

struct LegacyOnrampStateCleanupTests {
    private let settings = InMemorySettingsManager()

    @Test("the removed on-ramp's ids go, other settings stay")
    func removesRetiredOnrampIds() {
        for key in LegacyOnrampStateCleanup.keys {
            settings.set(value: ["id"], for: key)
        }
        settings.set(value: "alice", for: SettingsKey.username.rawValue)

        LegacyOnrampStateCleanup.run(settingsManager: settings)

        for key in LegacyOnrampStateCleanup.keys {
            #expect(settings.anyValue(for: key) == nil, "\(key) should be removed")
        }
        #expect(settings.string(for: SettingsKey.username.rawValue) == "alice")
    }
}
