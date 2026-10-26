import Foundation
import IssueMonitoring
import Keystore_iOS
import Testing
@testable import polkadot_app

@MainActor
@Suite("Game reminder delivery diagnostics")
struct GameReminderDeliveryDiagnosticsTests {
    private struct ScheduleFailed: Error {}

    private let productId = "game.dot"
    private let keys = GameReminderStorageKeys.product("game.dot")

    @available(iOS 26.1, *)
    private func makeAlarm(_ manager: MockGameAlarmManager, _ reporter: RecordingGameReminderIssueReporter)
        -> AlarmKitGameReminder {
        AlarmKitGameReminder(
            alarmManger: manager,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: reporter.diagnostics
        )
    }

    private func makeNotification(
        _ service: MockUserNotificationService,
        _ reporter: RecordingGameReminderIssueReporter
    ) -> LocalNotificationGameReminder {
        LocalNotificationGameReminder(
            localNotificationService: service,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: reporter.diagnostics
        )
    }

    @available(iOS 26.1, *)
    @Test func alarmBookedOnceIsReportedOnceAsScheduled() async throws {
        let manager = MockGameAlarmManager()
        let reporter = RecordingGameReminderIssueReporter()
        let reminder = makeAlarm(manager, reporter)
        let startsAt = Date().addingTimeInterval(3_600)

        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        try await waitUntil { manager.scheduled.count == 1 && !reporter.events.isEmpty }
        try await Task.sleep(for: .milliseconds(50))

        #expect(reporter.events.map(\.step) == [.scheduled])
        #expect(reporter.events.first?.delivery == .alarm)
        #expect(reporter.events.first?.startsAt == startsAt)
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
        let reporter = RecordingGameReminderIssueReporter()

        // The reminder holds itself only weakly while its work is queued.
        let reminder = makeAlarm(manager, reporter)
        reminder.scheduleReminder(gameDate: Date().addingTimeInterval(secondsToStart), productId: productId)
        try await waitUntil { !reporter.events.isEmpty }

        #expect(reporter.events.map(\.step) == [.notScheduled])
        #expect(reporter.events.first?.skipReason == reason)
        #expect(reporter.events.first?.delivery == nil)
    }

    @Test func notificationBookedIsReportedAsScheduled() async throws {
        let reporter = RecordingGameReminderIssueReporter()

        makeNotification(MockUserNotificationService(), reporter)
            .scheduleReminder(gameDate: Date().addingTimeInterval(3_600), productId: productId)
        try await waitUntil { !reporter.events.isEmpty }

        #expect(reporter.events.map(\.step) == [.scheduled])
        #expect(reporter.events.first?.delivery == .notification)
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
        let reporter = RecordingGameReminderIssueReporter()

        makeNotification(service, reporter).scheduleReminder(
            gameDate: Date().addingTimeInterval(secondsToStart),
            productId: productId
        )
        try await waitUntil { !reporter.events.isEmpty }

        #expect(reporter.events.map(\.step) == [.notScheduled])
        #expect(reporter.events.first?.skipReason == reason)
        #expect(reporter.events.first?.delivery == nil)
    }
}
