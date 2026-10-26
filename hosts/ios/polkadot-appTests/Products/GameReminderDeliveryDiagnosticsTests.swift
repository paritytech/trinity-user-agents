import Foundation
import Keystore_iOS
import Products
import Testing
@testable import polkadot_app

@MainActor
@Suite("Game reminder delivery diagnostics")
struct GameReminderDeliveryDiagnosticsTests {
    private struct ScheduleFailed: Error {}

    private let productId = "game.dot"
    private let keys = GameReminderStorageKeys.product("game.dot")

    @available(iOS 26.1, *)
    private func makeAlarm(_ manager: MockGameAlarmManager, _ recorder: RecordingGameReminderHostEventRecorder)
        -> AlarmKitGameReminder {
        AlarmKitGameReminder(
            alarmManger: manager,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: recorder.diagnostics
        )
    }

    private func makeNotification(
        _ service: MockUserNotificationService,
        _ recorder: RecordingGameReminderHostEventRecorder
    ) -> LocalNotificationGameReminder {
        LocalNotificationGameReminder(
            localNotificationService: service,
            settingsManager: InMemorySettingsManager(),
            keys: keys,
            diagnostics: recorder.diagnostics
        )
    }

    @available(iOS 26.1, *)
    @Test func alarmBookedOnceIsReportedOnceAsScheduled() async throws {
        let manager = MockGameAlarmManager()
        let recorder = RecordingGameReminderHostEventRecorder()
        let reminder = makeAlarm(manager, recorder)
        let startsAt = Date().addingTimeInterval(3_600)

        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        reminder.scheduleReminder(gameDate: startsAt, productId: productId)
        try await waitUntil { manager.scheduled.count == 1 && !recorder.events.isEmpty }
        try await Task.sleep(for: .milliseconds(50))

        #expect(recorder.events.map(\.step) == [.scheduled])
        #expect(recorder.events.first?.delivery == .alarm)
        let recordedStart = try #require(recorder.events.first?.startsAt)
        #expect(abs(recordedStart.timeIntervalSince(startsAt)) < 0.001)
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
        let recorder = RecordingGameReminderHostEventRecorder()

        // The reminder holds itself only weakly while its work is queued.
        let reminder = makeAlarm(manager, recorder)
        reminder.scheduleReminder(gameDate: Date().addingTimeInterval(secondsToStart), productId: productId)
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.step) == [.notScheduled])
        #expect(recorder.events.first?.skipReason == reason)
        #expect(recorder.events.first?.delivery == nil)
    }

    @Test func notificationBookedIsReportedAsScheduled() async throws {
        let recorder = RecordingGameReminderHostEventRecorder()

        makeNotification(MockUserNotificationService(), recorder)
            .scheduleReminder(gameDate: Date().addingTimeInterval(3_600), productId: productId)
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.step) == [.scheduled])
        #expect(recorder.events.first?.delivery == .notification)
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
        let recorder = RecordingGameReminderHostEventRecorder()

        makeNotification(service, recorder).scheduleReminder(
            gameDate: Date().addingTimeInterval(secondsToStart),
            productId: productId
        )
        try await waitUntil { !recorder.events.isEmpty }

        #expect(recorder.events.map(\.step) == [.notScheduled])
        #expect(recorder.events.first?.skipReason == reason)
        #expect(recorder.events.first?.delivery == nil)
    }
}
