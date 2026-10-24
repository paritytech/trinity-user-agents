import Foundation
import UserNotifications
import Keystore_iOS
import Products
import Individuality
import IssueMonitoring

final class LocalNotificationGameReminder {
    private let localNotificationService: UserNotificationServicing
    private let settingsManager: SettingsManagerProtocol
    private let keys: GameReminderStorageKeys
    private let logger: LoggerProtocol
    private let telemetry: GameReminderTelemetry

    init(
        localNotificationService: UserNotificationServicing,
        settingsManager: SettingsManagerProtocol,
        keys: GameReminderStorageKeys,
        logger: LoggerProtocol = Logger.shared,
        telemetry: GameReminderTelemetry = NoopGameReminderTelemetry()
    ) {
        self.localNotificationService = localNotificationService
        self.settingsManager = settingsManager
        self.keys = keys
        self.logger = logger
        self.telemetry = telemetry
    }
}

extension LocalNotificationGameReminder: GameStartReminderServicing {
    func scheduleReminder(gameDate: Date, productId: ProductId) {
        localNotificationService.notificationAccessStatus { status in
            guard status == .allowed else {
                self.record(.notScheduled, gameDate: gameDate, productId: productId, skipReason: .notAuthorized)
                return
            }
            self.performScheduleReminder(gameDate: gameDate, productId: productId)
        }
    }

    func cancelReminder() {
        cancel()
        settingsManager.removeValue(for: keys.notificationDate)
    }
}

private extension LocalNotificationGameReminder {
    func performScheduleReminder(gameDate: Date, productId: ProductId) {
        let notificationDate = Int(gameDate.timeIntervalSinceReferenceDate)
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
            record(.notScheduled, gameDate: fireDate, productId: productId, skipReason: .startInPast)
            return
        }

        settingsManager.set(value: notificationDate, for: keys.notificationDate)

        localNotificationService.scheduleNotification(
            withIdentifier: keys.notificationIdentifier,
            content: makeContent(productId: productId),
            after: fireDate.timeIntervalSince(Date())
        ) { [logger] error in
            logger.debug("gameStart scheduled with error: \(String(describing: error))")
            self.record(
                error == nil ? .scheduled : .notScheduled,
                gameDate: fireDate,
                productId: productId,
                skipReason: error == nil ? nil : .failed
            )
        }
    }

    func record(
        _ step: GameReminderEvent.Step,
        gameDate: Date,
        productId: ProductId,
        skipReason: GameReminderEvent.SkipReason? = nil
    ) {
        var event = GameReminderEvent(step: step, productId: productId, startsAt: gameDate)
        event.delivery = step == .scheduled ? .notification : nil
        event.skipReason = skipReason
        telemetry.record(event)
    }

    func cancel() {
        localNotificationService.cancelScheduledNotifications(withIdentifiers: [keys.notificationIdentifier])
    }

    func makeContent(productId: ProductId) -> UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = String(localized: .Notification.gameNotificationGameStartTitle)
        content.body = String(localized: .Notification.gameNotificationGameStartBody)
        // An ordinary notification: the core asks for this one when no alarm may ring.
        content.sound = .default
        // PushRouteBuilder opens a scheme-less product deeplink as https://<productId>.
        content.userInfo = [
            PushNotificationKeys.pushSource: PushNotificationSource.products.rawValue,
            PushNotificationKeys.deeplink: productId
        ]
        return content
    }
}
