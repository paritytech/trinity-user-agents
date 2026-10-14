import Foundation

protocol DeviceEncryptionStoring {
    var deviceEncryptId: String { get }
}

final class DeviceEncryptionStorage: DeviceEncryptionStoring {
    private static let key = SettingsKey.deviceEncryptId.rawValue

    private let userDefaults: UserDefaults

    init(userDefaults: UserDefaults = SharedContainerGroup.userDefaults) {
        self.userDefaults = userDefaults
    }

    var deviceEncryptId: String {
        if let existingId = userDefaults.string(forKey: Self.key) {
            return existingId
        }

        let newId = UUID().uuidString
        userDefaults.set(newId, forKey: Self.key)
        return newId
    }
}
