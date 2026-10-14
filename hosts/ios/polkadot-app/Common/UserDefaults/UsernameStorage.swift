import Foundation
import Keystore_iOS
import EventCenter

protocol UsernameStoring: AnyObject {
    var username: Username? { get set }
    var usernameClaimed: Bool { get set }
}

extension UsernameStoring {
    var hasUsername: Bool {
        username != nil
    }
}

final class UsernameStorage {
    private let settingsManager: SettingsManagerProtocol
    private let eventCenter: EventCenterProtocol

    init(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        eventCenter: EventCenterProtocol = EventCenter.shared
    ) {
        self.settingsManager = settingsManager
        self.eventCenter = eventCenter
    }
}

extension UsernameStorage: UsernameStoring {
    var username: Username? {
        get {
            guard let value = usernameValue, !value.isEmpty else {
                return nil
            }
            return Username(value: value)
        }
        set {
            usernameValue = newValue?.value
            eventCenter.notify(with: SelectedUsernameChanged(username: newValue))
        }
    }

    var usernameClaimed: Bool {
        get {
            settingsManager.bool(for: SettingsKey.usernameClaimed.rawValue) ?? false
        } set {
            settingsManager.set(value: newValue, for: SettingsKey.usernameClaimed.rawValue)
        }
    }
}

private extension UsernameStorage {
    var usernameValue: String? {
        get {
            settingsManager.string(for: SettingsKey.username.rawValue)
        }
        set {
            if let value = newValue {
                settingsManager.set(value: value, for: SettingsKey.username.rawValue)
            } else {
                settingsManager.removeValue(for: SettingsKey.username.rawValue)
            }
        }
    }
}
