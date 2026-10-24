import Foundation
import IssueMonitoring
import Keystore_iOS
import Testing
@testable import polkadot_app

@MainActor
@Suite("Game reminder delivery telemetry")
struct GameReminderDeliveryTelemetryTests {
    private struct ScheduleFailed: Error {}

    private let productId = "game.dot"
    private let keys = GameReminderStorageKeys.product("game.dot")

    @available(iOS 26.1, *)
    private func makeAlarm(_ manager: MockGameAlarmManager, _ telemetry: RecordingGameReminderTelemetry)
        -> AlarmKitGameReminder {
        AlarmKitGameReminder(
            alarmManger: manager,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            telemetry: telemetry
        )
    }

    private func makeNotification(
        _ service: MockUserNotificationService,
        _ telemetry: RecordingGameReminderTelemetry
    ) -> LocalNotificationGameReminder {
        LocalNotificationGameReminder(
            localNotificationService: service,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            telemetry: telemetry
        )
    }

    @available(iOS 26.1, *)
    @Test func alarmBookedOnceIsReportedOnceAsScheduled() async throws {
        let manager = MockGameAlarmManager()
        let telemetry = RecordingGameReminderTelemetry()
        let reminder = makeAlarm(manager, telemetry)
        let startsAt = Date().addingTimeInterval(3_600)

        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        try await waitUntil { manager.scheduled.count == 1 && !telemetry.events.isEmpty }
        try await Task.sleep(for: .milliseconds(50))

        #expect(telemetry.events.map(\.step) == [.scheduled])
        #expect(telemetry.events.first?.delivery == .alarm)
        #expect(telemetry.events.first?.startsAt == startsAt)
    }

    @Test(
        "An alarm that is not booked says why",
        arguments: [
            (false, 3_600.0, false, GameReminderEvent.SkipReason.notAuthorized),
            (true, -60.0, false, .startInPast),
            (true, 3_600.0, true, .failed)
        ]
    )
    @available(iOS 26.1, *)
    func alarmNotScheduled(
        authorized: Bool,
        secondsToStart: TimeInterval,
        fails: Bool,
        reason: GameReminderEvent.SkipReason
    ) async throws {
        let manager = MockGameAlarmManager()
        manager.isAuthorized = authorized
        manager.scheduleError = fails ? ScheduleFailed() : nil
        let telemetry = RecordingGameReminderTelemetry()

        // The reminder holds itself only weakly while its work is queued.
        let reminder = makeAlarm(manager, telemetry)
        reminder.scheduleReminder(gameDate: Date().addingTimeInterval(secondsToStart), productId: productId)
        try await waitUntil { !telemetry.events.isEmpty }

        #expect(telemetry.events.map(\.step) == [.notScheduled])
        #expect(telemetry.events.first?.skipReason == reason)
        #expect(telemetry.events.first?.delivery == nil)
    }

    @Test func notificationBookedIsReportedAsScheduled() async throws {
        let telemetry = RecordingGameReminderTelemetry()

        makeNotification(MockUserNotificationService(), telemetry)
            .scheduleReminder(gameDate: Date().addingTimeInterval(3_600), productId: productId)
        try await waitUntil { !telemetry.events.isEmpty }

        #expect(telemetry.events.map(\.step) == [.scheduled])
        #expect(telemetry.events.first?.delivery == .notification)
    }

    @Test(
        "A notification that is not booked says why",
        arguments: [
            (false, 3_600.0, false, GameReminderEvent.SkipReason.notAuthorized),
            (true, -60.0, false, .startInPast),
            (true, 3_600.0, true, .failed)
        ]
    )
    func notificationNotScheduled(
        allowed: Bool,
        secondsToStart: TimeInterval,
        fails: Bool,
        reason: GameReminderEvent.SkipReason
    ) async throws {
        let service = MockUserNotificationService()
        service.accessStatus = allowed ? .allowed : .notAllowed(denied: true)
        service.scheduleError = fails ? ScheduleFailed() : nil
        let telemetry = RecordingGameReminderTelemetry()

        makeNotification(service, telemetry).scheduleReminder(
            gameDate: Date().addingTimeInterval(secondsToStart),
            productId: productId
        )
        try await waitUntil { !telemetry.events.isEmpty }

        #expect(telemetry.events.map(\.step) == [.notScheduled])
        #expect(telemetry.events.first?.skipReason == reason)
        #expect(telemetry.events.first?.delivery == nil)
    }
}
