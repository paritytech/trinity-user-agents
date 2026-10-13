import Foundation
import Individuality
import Products

/// Separate keys let each product's reminder coexist without cancelling the others.
struct GameReminderStorageKeys {
    let alarmId: String
    let alarmFireDate: String
    let notificationDate: String
    let notificationIdentifier: String

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
    func scheduleReminder(gameDate: Date, productId: ProductId, timingSeconds: Int)
    func cancelReminder()
}
