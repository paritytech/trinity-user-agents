import Foundation
import FoundationExt
import Keystore_iOS
import PolkadotUI
import Products
import UIKit
@testable import polkadot_app

final class GameReminderCenterFakes: @unchecked Sendable {
    var now: Date
    var applicationState: UIApplication.State = .active
    var opened: [String] = []
    var openNavigates = true
    let calendar = RecordingCalendar()
    let recorder = RecordingHostEventRecorder()
    var currentAlertingAlarms: Set<UUID> = []
    var diagnostics: GameReminderDiagnostics { .recording(into: recorder) }
    let alarmAlerts: AsyncStream<Set<UUID>>.Continuation
    private let alarmAlertStream: AsyncStream<Set<UUID>>
    private let alarmAvailable: Bool
    private var alarms: [String: RecordingGameReminder] = [:]
    private var notifications: [String: RecordingGameReminder] = [:]

    init(now: Date, alarmAvailable: Bool) {
        self.now = now
        self.alarmAvailable = alarmAvailable
        (alarmAlertStream, alarmAlerts) = AsyncStream.makeStream()
    }

    func alarm(for productId: String) -> RecordingGameReminder {
        reminder(in: &alarms, for: productId)
    }

    func notification(for productId: String) -> RecordingGameReminder {
        reminder(in: &notifications, for: productId)
    }

    private func reminder(
        in store: inout [String: RecordingGameReminder],
        for productId: String
    ) -> RecordingGameReminder {
        if let reminder = store[productId] {
            return reminder
        }
        let reminder = RecordingGameReminder()
        store[productId] = reminder
        return reminder
    }
}

extension GameReminderCenterFakes: ProductGameReminderServicesMaking, ApplicationStateProviding, ProductOpening,
    CurrentDateProviding {
    func makeAlarm(for productId: String) -> (any GameStartReminderServicing)? {
        alarmAvailable ? alarm(for: productId) : nil
    }

    func makeNotification(for productId: String) -> any GameStartReminderServicing {
        notification(for: productId)
    }

    func makeCalendar() -> any GameCalendarServicing {
        calendar
    }

    func alertingAlarms() -> AsyncStream<Set<UUID>> {
        alarmAlertStream
    }

    func currentAlertingAlarmIds() -> Set<UUID> {
        currentAlertingAlarms
    }

    func open(productId: String) -> Bool {
        opened.append(productId)
        return openNavigates
    }
}

@MainActor
extension GameReminderCenterFakes {
    static func makeCenter(
        now: Date,
        alarmAvailable: Bool = true,
        settings: SettingsManagerProtocol = InMemorySettingsManager()
    ) -> (center: ProductGameReminderCenter, fakes: GameReminderCenterFakes) {
        let fakes = GameReminderCenterFakes(now: now, alarmAvailable: alarmAvailable)
        let center = ProductGameReminderCenter(
            dependencies: .init(
                services: fakes,
                settingsManager: settings,
                applicationState: fakes,
                applicationStateStreams: ApplicationStateStreamFactory(),
                productOpener: fakes,
                dateProvider: fakes,
                diagnostics: fakes.diagnostics
            )
        )
        center.start()
        return (center, fakes)
    }
}

extension ProductGameReminderCenter {
    func schedule(productId: String, startsAt: Date, addCalendarEvent: Bool) async {
        await schedule(productId: productId, startsAt: startsAt, ringAlarm: true, addCalendarEvent: addCalendarEvent)
    }
}
