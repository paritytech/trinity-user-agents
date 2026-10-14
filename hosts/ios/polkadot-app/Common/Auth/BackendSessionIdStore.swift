import Foundation
import Keystore_iOS

protocol BackendSessionIdStoring {
    func getOrCreateSessionId() -> String
}

final class BackendSessionIdStore: BackendSessionIdStoring {
    private let settingsManager: SettingsManagerProtocol

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager
    }

    func getOrCreateSessionId() -> String {
        if let existingId = settingsManager.string(for: SettingsKey.backendSessionId.rawValue) {
            return existingId
        }

        let newId = UUID().uuidString
        settingsManager.set(value: newId, for: SettingsKey.backendSessionId.rawValue)
        return newId
    }
}
