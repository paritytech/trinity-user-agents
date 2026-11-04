import struct TrUAPIHost.ProductId
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
    var opened: [ProductId] = []
    var openNavigates = true
    let calendar = RecordingCalendar()
    let recorder = RecordingHostEventRecorder()
    var currentAlertingAlarms: Set<UUID> = []
    private(set) var alertingAlarmReads = 0
    private let isReporting: Bool
    var diagnostics: GameReminderDiagnostics {
        .recording(
            into: recorder,
            now: { self.now },
            applicationState: { self.applicationState },
            isReporting: isReporting
        )
    }

    let alarmAlerts: AsyncStream<Set<UUID>>.Continuation
    private let alarmAlertStream: AsyncStream<Set<UUID>>
    private let alarmAvailable: Bool
    private var alarms: [ProductId: RecordingGameReminder] = [:]
    private var notifications: [ProductId: RecordingGameReminder] = [:]

    init(now: Date, alarmAvailable: Bool, isReporting: Bool) {
        self.now = now
        self.isReporting = isReporting
        self.alarmAvailable = alarmAvailable
        (alarmAlertStream, alarmAlerts) = AsyncStream.makeStream()
    }

    func alarm(for productId: ProductId) -> RecordingGameReminder {
        reminder(in: &alarms, for: productId)
    }

    func notification(for productId: ProductId) -> RecordingGameReminder {
        reminder(in: &notifications, for: productId)
    }

    private func reminder(
        in store: inout [ProductId: RecordingGameReminder],
        for productId: ProductId
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
    func makeAlarm(for productId: ProductId) -> (any GameStartReminderServicing)? {
        alarmAvailable ? alarm(for: productId) : nil
    }

    func makeNotification(for productId: ProductId) -> any GameStartReminderServicing {
        notification(for: productId)
    }

    func makeCalendar() -> any GameCalendarServicing {
        calendar
    }

    func alertingAlarms() -> AsyncStream<Set<UUID>> {
        alarmAlertStream
    }

    func currentAlertingAlarmIds() -> Set<UUID> {
        alertingAlarmReads += 1
        return currentAlertingAlarms
    }

    func open(productId: ProductId) -> Bool {
        opened.append(productId)
        return openNavigates
    }
}

@MainActor
extension GameReminderCenterFakes {
    static func makeCenter(
        now: Date,
        alarmAvailable: Bool = true,
        isReporting: Bool = true,
        settings: SettingsManagerProtocol = InMemorySettingsManager()
    ) -> (center: ProductGameReminderCenter, fakes: GameReminderCenterFakes) {
        let fakes = GameReminderCenterFakes(now: now, alarmAvailable: alarmAvailable, isReporting: isReporting)
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
    func schedule(productId: ProductId, startsAt: Date, addCalendarEvent: Bool) async {
        await schedule(productId: productId, startsAt: startsAt, ringAlarm: true, addCalendarEvent: addCalendarEvent)
    }
}
