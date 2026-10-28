import Foundation
import Keystore_iOS

/// Removes the on-ramp ids the removed Meld integration left in UserDefaults.
///
/// Their `SettingsKey` cases are gone, so nothing reads or clears these raw names any more and an
/// upgraded install would keep them for good. Runs on every launch: a second run finds nothing to remove.
enum LegacyOnrampStateCleanup {
    static let keys = ["fiatOnrampSessionIds", "fiatOnrampTrackedTransactionIds"]

    static func run(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        for key in keys {
            settingsManager.removeValue(for: key)
        }
    }
}
