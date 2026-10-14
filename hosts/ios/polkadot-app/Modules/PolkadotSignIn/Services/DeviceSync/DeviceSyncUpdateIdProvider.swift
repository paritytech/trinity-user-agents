import Foundation
import Keystore_iOS

protocol DeviceSyncUpdateIdProviding {
    func nextId() -> UInt32
}

/// Persisted monotonic counter for outgoing `SyncUpdate.id`s.
/// Survives process restart so update ids stay strictly increasing across sessions.
final class DeviceSyncUpdateIdProvider: DeviceSyncUpdateIdProviding {
    private let settingsManager: SettingsManagerProtocol
    private let lock = NSLock()

    init(settingsManager: SettingsManagerProtocol = SettingsManager.shared) {
        self.settingsManager = settingsManager
    }

    func nextId() -> UInt32 {
        lock.withLock {
            let current = settingsManager.integer(for: .nextSyncUpdateId) ?? 0
            let next = current + 1
            settingsManager.set(value: next, for: .nextSyncUpdateId)
            return UInt32(next)
        }
    }
}
