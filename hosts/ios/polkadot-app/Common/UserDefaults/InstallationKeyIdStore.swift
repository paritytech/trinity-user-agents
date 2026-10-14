import Foundation
import KeyDerivation

/// Persists the installation key id in the App Group suite, where the Keychain-indexing ids live.
/// `UserDefaults` is thread-safe, hence the unchecked conformance.
final class InstallationKeyIdStore: InstallationKeyIdStoring, @unchecked Sendable {
    private static let key = SettingsKey.installationKeyId.rawValue

    private let userDefaults: UserDefaults

    init(userDefaults: UserDefaults = SharedContainerGroup.userDefaults) {
        self.userDefaults = userDefaults
    }

    func saveInstallationKeyId(_ installationKeyId: String) {
        userDefaults.set(installationKeyId, forKey: Self.key)
        userDefaults.synchronize()
    }

    func getInstallationKeyId() -> String? {
        userDefaults.string(forKey: Self.key)
    }
}
