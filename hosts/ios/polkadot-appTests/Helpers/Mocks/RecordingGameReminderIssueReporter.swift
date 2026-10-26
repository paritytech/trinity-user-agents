import Foundation
import IssueMonitoring
import os

final class RecordingGameReminderIssueReporter: IssueReporting, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[GameReminderEvent]>(initialState: [])

    var events: [GameReminderEvent] { state.withLock { $0 } }

    func report(_: CriticalIssue) {}

    func report(_ event: GameReminderEvent) {
        state.withLock { $0.append(event) }
    }

    var diagnostics: GameReminderDiagnostics {
        GameReminderDiagnostics(reporter: self)
    }
}
