import Foundation
import Products
@preconcurrency import Keystore_iOS

final class SettingsDotNsTldStore: DotNsTldStoring {
    private let settingsManager: SettingsManagerProtocol
    private static let key = "dotNsTld"

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager
    }

    func loadTld() -> String? {
        settingsManager.string(for: Self.key)
    }

    func saveTld(_ tld: String) {
        settingsManager.set(value: tld, for: Self.key)
    }
}
