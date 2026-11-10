import Foundation
import Keystore_iOS

struct GameCalendarReminder: Codable, Equatable {
    let title: String
    let startDate: Date
    let endDate: Date
}

extension SettingsManagerProtocol {
    var gameCalendarReminder: GameCalendarReminder? {
        get {
            guard let data = anyValue(for: SettingsKey.gameCalendarReminder.rawValue) as? Data else {
                return nil
            }
            return try? JSONDecoder().decode(GameCalendarReminder.self, from: data)
        }
        set {
            guard let newValue, let data = try? JSONEncoder().encode(newValue) else {
                removeValue(for: SettingsKey.gameCalendarReminder.rawValue)
                return
            }
            set(anyValue: data, for: SettingsKey.gameCalendarReminder.rawValue)
        }
    }
}
