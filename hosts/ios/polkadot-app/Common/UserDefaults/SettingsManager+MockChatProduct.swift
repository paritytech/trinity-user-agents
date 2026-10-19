import Foundation
import Keystore_iOS

extension SettingsManagerProtocol {
    /// Whether the chat list shows the rooms of the chat product that lives in the app (`MockChatProduct`). Only
    /// builds with `TESTNET_FEATURE` have that product.
    var isMockChatProductEnabled: Bool {
        bool(for: SettingsKey.mockChatProductEnabled.rawValue) ?? false
    }
}
