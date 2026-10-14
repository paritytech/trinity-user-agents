import Foundation
import Keystore_iOS

extension SettingsManagerProtocol {
    private func getKey(extId: ChatExtension.Id) -> String {
        "welcome-v3:\(extId)"
    }

    func hasWelcomeMessage(from extId: ChatExtension.Id) -> Bool {
        bool(for: getKey(extId: extId)) ?? false
    }

    func markWelcomeMessageSent(from extId: ChatExtension.Id) {
        set(value: true, for: getKey(extId: extId))
    }
}
