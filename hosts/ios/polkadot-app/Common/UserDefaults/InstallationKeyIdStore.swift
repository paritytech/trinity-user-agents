import Foundation
import KeyDerivation

/// Persists the installation key id in the App Group suite, where the Keychain-indexing ids live.
/// `UserDefaults` is thread-safe, hence the unchecked conformance.
final class InstallationKeyIdStore: InstallationKeyIdStoring, @unchecked Sendable {
    static let willChangeNotification = Notification.Name("InstallationKeyIdWillChange")
    static let didChangeNotification = Notification.Name("InstallationKeyIdDidChange")

    private static let key = SettingsKey.installationKeyId.rawValue

    private let userDefaults: UserDefaults

    init(userDefaults: UserDefaults = SharedContainerGroup.userDefaults) {
        self.userDefaults = userDefaults
    }

    func saveInstallationKeyId(_ installationKeyId: String) {
        NotificationCenter.default.post(name: Self.willChangeNotification, object: nil)
        userDefaults.set(installationKeyId, forKey: Self.key)
        userDefaults.synchronize()
        NotificationCenter.default.post(name: Self.didChangeNotification, object: nil)
    }

    func getInstallationKeyId() -> String? {
        userDefaults.string(forKey: Self.key)
    }
}
