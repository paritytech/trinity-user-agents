import Foundation
import Keystore_iOS

protocol ThemeSelectionStoring {
    var hasSelectedTheme: Bool { get }
    func setSelected()
}

final class ThemeSelectionStorage: ThemeSelectionStoring {
    private let settingsManager: SettingsManagerProtocol

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager
    }

    var hasSelectedTheme: Bool {
        settingsManager.value(for: .themeSelected)
    }

    func setSelected() {
        settingsManager.set(value: true, for: .themeSelected)
    }
}
