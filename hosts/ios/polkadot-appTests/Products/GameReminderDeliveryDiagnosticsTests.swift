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

    /// The reminder is held only weakly while its work is queued, so callers keep it in a `let`.
    @available(iOS 26.1, *)
    private func makeAlarm(_ manager: MockGameAlarmManager, _ recorder: RecordingHostEventRecorder)
        -> AlarmKitGameReminder {
        AlarmKitGameReminder(
            alarmManger: manager,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: .recording(into: recorder)
        )
    }

    private func makeNotification(
        _ service: MockUserNotificationService,
        _ recorder: RecordingHostEventRecorder
    ) -> LocalNotificationGameReminder {
        LocalNotificationGameReminder(
            localNotificationService: service,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: .recording(into: recorder)
        )
    }

    @available(iOS 26.1, *)
    @Test func alarmBookedIsReportedAsScheduled() async throws {
        let manager = MockGameAlarmManager()
        let recorder = RecordingHostEventRecorder()
        let startsAt = Date().addingTimeInterval(3_600)

        let reminder = makeAlarm(manager, recorder)
        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.name) == ["host.reminder.scheduled"])
        #expect(recorder.events.first?.tags["reminder.delivery"] == "alarm")
        #expect(recorder.events.first?.extras["reminder.starts_at_unix_ms"]
            == .integer(Int(startsAt.timeIntervalSince1970 * 1_000)))
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
        let recorder = RecordingHostEventRecorder()

        let reminder = makeAlarm(manager, recorder)
        reminder.scheduleReminder(gameDate: Date().addingTimeInterval(secondsToStart), productId: productId)
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.name) == ["host.reminder.not_scheduled"])
        #expect(recorder.events.first?.tags["reminder.skip_reason"] == reason.rawValue)
    }

    @Test func notificationBookedIsReportedAsScheduled() async throws {
        let recorder = RecordingHostEventRecorder()

        makeNotification(MockUserNotificationService(), recorder)
            .scheduleReminder(gameDate: Date().addingTimeInterval(3_600), productId: productId)
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.name) == ["host.reminder.scheduled"])
        #expect(recorder.events.first?.tags["reminder.delivery"] == "notification")
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
        let recorder = RecordingHostEventRecorder()

        makeNotification(service, recorder).scheduleReminder(
            gameDate: Date().addingTimeInterval(secondsToStart),
            productId: productId
        )
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.name) == ["host.reminder.not_scheduled"])
        #expect(recorder.events.first?.tags["reminder.skip_reason"] == reason.rawValue)
    }
}
