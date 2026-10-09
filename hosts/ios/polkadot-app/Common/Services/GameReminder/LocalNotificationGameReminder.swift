import Foundation
import UserNotifications
import Keystore_iOS
import Individuality

final class LocalNotificationGameReminder {
    private let localNotificationService: UserNotificationServicing
    private let settingsManager: SettingsManagerProtocol
    private let keys: GameReminderStorageKeys
    private let logger: LoggerProtocol

    init(
        localNotificationService: UserNotificationServicing,
        settingsManager: SettingsManagerProtocol,
        keys: GameReminderStorageKeys = .game,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.localNotificationService = localNotificationService
        self.settingsManager = settingsManager
        self.keys = keys
        self.logger = logger
    }
}

extension LocalNotificationGameReminder: GameStartReminderServicing {
    func scheduleReminder(gameDate: Date, target: GameReminderTarget, timingSeconds: Int) {
        localNotificationService.notificationAccessStatus { status in
            guard status == .allowed else {
                return
            }
            self.performScheduleReminder(gameDate: gameDate, target: target, timingSeconds: timingSeconds)
        }
    }

    func cancelReminder() {
        cancel()
        settingsManager.removeValue(for: keys.notificationDate)
    }
}

private extension LocalNotificationGameReminder {
    func performScheduleReminder(gameDate: Date, target: GameReminderTarget, timingSeconds: Int) {
        let notificationDate = Int(gameDate.timeIntervalSinceReferenceDate) - timingSeconds
        let storedDate = settingsManager.integer(for: keys.notificationDate)

        guard notificationDate != storedDate else {
            logger.debug("gameStart date unchanged, skipping reschedule")
            return
        }

        cancel()
        settingsManager.removeValue(for: keys.notificationDate)

        let fireDate = Date(timeIntervalSinceReferenceDate: TimeInterval(notificationDate))

        guard fireDate > Date() else {
            logger.debug("fire date is in the past, skipping")
            return
        }

        settingsManager.set(value: notificationDate, for: keys.notificationDate)

        localNotificationService.scheduleNotification(
            withIdentifier: keys.notificationIdentifier,
            content: makeContent(timingSeconds: timingSeconds, target: target),
            after: fireDate.timeIntervalSince(Date())
        ) { [logger] error in
            logger.debug("gameStart scheduled with error: \(String(describing: error))")
        }
    }

    func cancel() {
        localNotificationService.cancelScheduledNotifications(withIdentifiers: [keys.notificationIdentifier])
    }

    func makeContent(timingSeconds: Int, target: GameReminderTarget) -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = String(
            localized: .Notification.gameNotificationGameStartTitle(String(timingSeconds))
        )
        content.body = String(localized: .Notification.gameNotificationGameStartBody)
        switch target {
        case let .game(gameIndex):
            content.sound = UNNotificationSound(named: .init("game_alarm.caf"))
            content.userInfo = [
                PushNotificationKeys.chatExtensionId: DIM2ChatExtension.identifier,
                PushNotificationKeys.pushSource: PushNotificationSource.chat.rawValue,
                PushNotificationKeys.gameState: PushGameNotificationType.start.rawValue,
                PushNotificationKeys.gameIndex: Int(gameIndex)
            ]
        case let .product(productId):
            // An ordinary notification: the core asks for this one when no alarm may ring.
            content.sound = .default
            // PushRouteBuilder opens a scheme-less product deeplink as https://<productId>.
            content.userInfo = [
                PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
                PushNotificationKeys.deeplink: productId
            ]
        }
        return content
    }
}
