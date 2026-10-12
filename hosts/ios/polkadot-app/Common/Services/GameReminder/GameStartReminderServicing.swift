import Foundation
import Individuality
import Products

enum GameReminderTarget: Equatable {
    case game(GamePallet.GameIndex)
    case product(ProductId)
}

/// Separate keys let the native game and each product's reminder coexist without cancelling each other.
struct GameReminderStorageKeys {
    let alarmId: String
    let alarmFireDate: String
    let notificationDate: String
    let notificationIdentifier: String

    static let game = GameReminderStorageKeys(
        alarmId: SettingsKey.gameAlarmId.rawValue,
        alarmFireDate: SettingsKey.gameAlarmFireDate.rawValue,
        notificationDate: SettingsKey.gameStartNotificationDate.rawValue,
        notificationIdentifier: "game_start"
    )

    static func product(_ productId: ProductId) -> GameReminderStorageKeys {
        GameReminderStorageKeys(
            alarmId: "productGameAlarmId.\(productId)",
            alarmFireDate: "productGameAlarmFireDate.\(productId)",
            notificationDate: "productGameStartNotificationDate.\(productId)",
            notificationIdentifier: "product_game_start.\(productId)"
        )
    }
}

protocol GameStartReminderServicing {
    func scheduleReminder(gameDate: Date, target: GameReminderTarget, timingSeconds: Int)
    func cancelReminder()
}
