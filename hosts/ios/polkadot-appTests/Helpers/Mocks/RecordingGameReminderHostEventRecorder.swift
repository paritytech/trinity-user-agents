import Foundation
import IssueMonitoring
import Products
import os

struct RecordedGameReminderEvent {
    let step: GameReminderEvent.Step
    let productId: String
    let startsAt: Date?
    let occurredAt: Date?
    let delivery: GameReminderEvent.Delivery?
    let skipReason: GameReminderEvent.SkipReason?
    let openSource: GameReminderEvent.OpenSource?
    let appState: GameReminderEvent.AppState?
    let navigated: Bool?

    init(_ event: HostEvent) {
        step = GameReminderEvent.Step(rawValue: String(event.name.dropFirst("host.reminder.".count)))!
        productId = event.tags["reminder.product_id"]!
        startsAt = event.extras["reminder.starts_at_unix_ms"].flatMap { value in
            guard case let .integer(milliseconds) = value else { return nil }
            return Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
        }
        occurredAt = startsAt.flatMap { startsAt in
            event.extras["reminder.delay_after_start_ms"].flatMap { value in
                guard case let .integer(milliseconds) = value else { return nil }
                return startsAt.addingTimeInterval(Double(milliseconds) / 1_000)
            }
        }
        delivery = event.tags["reminder.delivery"].flatMap(GameReminderEvent.Delivery.init(rawValue:))
        skipReason = event.tags["reminder.skip_reason"].flatMap(GameReminderEvent.SkipReason.init(rawValue:))
        openSource = event.tags["reminder.open_source"].flatMap(GameReminderEvent.OpenSource.init(rawValue:))
        appState = event.tags["host.app_state"].flatMap(GameReminderEvent.AppState.init(rawValue:))
        if case let .boolean(value) = event.extras["reminder.navigated"] {
            navigated = value
        } else {
            navigated = nil
        }
    }
}

final class RecordingGameReminderHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[RecordedGameReminderEvent]>(initialState: [])

    var events: [RecordedGameReminderEvent] { state.withLock { $0 } }

    func record(_ event: @autoclosure () -> HostEvent) {
        let event = event()
        state.withLock { $0.append(RecordedGameReminderEvent(event)) }
    }

    var diagnostics: GameReminderDiagnostics {
        GameReminderDiagnostics(
            recorder: self,
            origin: HostEventOrigin(
                launchID: UUID(),
                osVersion: "test",
                appVersion: nil,
                appBuild: nil,
                appVariant: "test"
            )
        )
    }
}
