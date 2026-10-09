import AlarmKit
import Foundation
import Keystore_iOS

/// Stops the reminder the native weekly game left behind.
///
/// That game is gone, but an alarm it scheduled stays with AlarmKit until
/// something cancels it, and nothing else holds its id any more. Reuses the
/// reminders so the stored keys are read and cleared the way they were written.
/// Doing this twice is harmless: the second run finds nothing to cancel.
enum LegacyGameAlarmCleanup {
    private static let keys = GameReminderStorageKeys(
        alarmId: "gameAlarmId",
        alarmFireDate: "gameAlarmFireDate",
        notificationDate: "gameStartNotificationDate",
        notificationIdentifier: "game_start"
    )

    static func run(
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        localNotificationService: UserNotificationServicing = UserNotificationService.shared
    ) {
        LocalNotificationGameReminder(
            localNotificationService: localNotificationService,
            settingsManager: settingsManager,
            keys: keys
        ).cancelReminder()

        if #available(iOS 26.1, *) {
            AlarmKitGameReminder(
                alarmManger: .shared,
                settingsManager: settingsManager,
                keys: keys
            ).cancelReminder()
        }
    }
}
