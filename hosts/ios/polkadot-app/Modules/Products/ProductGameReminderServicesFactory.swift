import EventKit
import Foundation
import Keystore_iOS
import Products

/// Builds the services a product's game reminder is delivered through.
protocol ProductGameReminderServicesMaking {
    /// Nil when AlarmKit is unavailable on this OS.
    func makeAlarm(for productId: ProductId) -> (any GameStartReminderServicing)?
    func makeNotification(for productId: ProductId) -> any GameStartReminderServicing
    /// Built at add time, so its store sees the current calendar grant.
    func makeCalendar() -> any GameCalendarServicing
}

struct ProductGameReminderServicesFactory: ProductGameReminderServicesMaking {
    let settingsManager: SettingsManagerProtocol
    let localNotificationService: UserNotificationServicing

    func makeAlarm(for productId: ProductId) -> (any GameStartReminderServicing)? {
        if #available(iOS 26.1, *) {
            return AlarmKitGameReminder(
                alarmManger: .shared,
                settingsManager: settingsManager,
                keys: .product(productId)
            )
        }
        return nil
    }

    func makeNotification(for productId: ProductId) -> any GameStartReminderServicing {
        LocalNotificationGameReminder(
            localNotificationService: localNotificationService,
            settingsManager: settingsManager,
            keys: .product(productId)
        )
    }

    func makeCalendar() -> any GameCalendarServicing {
        GameCalendarService(eventStore: EKEventStore(), settings: settingsManager)
    }
}
