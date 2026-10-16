import Foundation
import Keystore_iOS
import Testing

@testable import polkadot_app

struct LocalStateEraserTests {
    private let settings = InMemorySettingsManager()

    @Test("the previous wallet's identity and progress go, device preferences stay")
    func erasesWalletStateOnly() {
        settings.set(value: "alice", for: SettingsKey.username.rawValue)
        settings.set(value: true, for: SettingsKey.usernameClaimed.rawValue)
        settings.set(value: "session", for: SettingsKey.backendSessionId.rawValue)
        settings.set(value: 7, for: SettingsKey.nextSyncUpdateId.rawValue)
        settings.set(value: Data([0x01]), for: SettingsKey.fiatOnrampSessionIds.rawValue)
        settings.set(value: Data([0x02]), for: SettingsKey.fiatOnrampTrackedTransactionIds.rawValue)
        settings.set(value: "dark", for: SettingsKey.themeSelected.rawValue)
        settings.set(value: "USD", for: SettingsKey.selectedCurrencyCode.rawValue)
        settings.set(value: true, for: SettingsKey.playerTooltipShown.rawValue)

        LocalStateEraser(settingsManager: settings, logger: StubLogger()).eraseUserState()

        for key in LocalStateEraser.identityKeys + LocalStateEraser.walletProgressKeys {
            #expect(settings.anyValue(for: key.rawValue) == nil, "\(key) should be erased")
        }
        #expect(settings.string(for: SettingsKey.themeSelected.rawValue) == "dark")
        #expect(settings.string(for: SettingsKey.selectedCurrencyCode.rawValue) == "USD")
        #expect(settings.bool(for: SettingsKey.playerTooltipShown.rawValue) == true)
    }
}
