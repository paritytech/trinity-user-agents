import AlarmKit
import EventKit
import Foundation
import IssueMonitoring
import Keystore_iOS
import Products

/// Builds the services a product's game reminder is delivered through.
protocol ProductGameReminderServicesMaking {
    /// Nil when AlarmKit is unavailable on this OS.
    func makeAlarm(for productId: ProductId) -> (any GameStartReminderServicing)?
    func makeNotification(for productId: ProductId) -> any GameStartReminderServicing
    /// Built at add time, so its store sees the current calendar grant.
    func makeCalendar() -> any GameCalendarServicing
    /// The ids of the alarms ringing now, each time AlarmKit reports a change while the app runs. Never yields
    /// without AlarmKit.
    func alertingAlarms() -> AsyncStream<Set<UUID>>
}

struct ProductGameReminderServicesFactory: ProductGameReminderServicesMaking {
    let settingsManager: SettingsManagerProtocol
    let localNotificationService: UserNotificationServicing
    let telemetry: GameReminderTelemetry

    func makeAlarm(for productId: ProductId) -> (any GameStartReminderServicing)? {
        if #available(iOS 26.1, *) {
            return AlarmKitGameReminder(
                alarmManger: AlarmManager.shared,
                settingsManager: settingsManager,
                keys: .product(productId),
                telemetry: telemetry
            )
        }
        return nil
    }

    func makeNotification(for productId: ProductId) -> any GameStartReminderServicing {
        LocalNotificationGameReminder(
            localNotificationService: localNotificationService,
            settingsManager: settingsManager,
            keys: .product(productId),
            telemetry: telemetry
        )
    }

    func makeCalendar() -> any GameCalendarServicing {
        GameCalendarService(eventStore: EKEventStore(), settings: settingsManager)
    }

    func alertingAlarms() -> AsyncStream<Set<UUID>> {
        guard #available(iOS 26.1, *) else {
            return AsyncStream { $0.finish() }
        }
        let updates = AlarmManager.shared.alarmUpdates
        return AsyncStream { continuation in
            let task = Task {
                for await alarms in updates {
                    continuation.yield(Set(alarms.filter { $0.state == .alerting }.map(\.id)))
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
