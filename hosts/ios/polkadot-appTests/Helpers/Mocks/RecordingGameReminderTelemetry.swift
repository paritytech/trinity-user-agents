import Foundation
import IssueMonitoring
import os

final class RecordingGameReminderTelemetry: GameReminderTelemetry, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[GameReminderEvent]>(initialState: [])

    var events: [GameReminderEvent] { state.withLock { $0 } }

    func record(_ event: GameReminderEvent) {
        state.withLock { $0.append(event) }
    }
}
