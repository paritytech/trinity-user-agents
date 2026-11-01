import Foundation
import IssueMonitoring
import os

final class RecordingIssueReporter: IssueReporting, @unchecked Sendable {
    private let issues = OSAllocatedUnfairLock(initialState: [CriticalIssue]())

    var reported: [CriticalIssue] { issues.withLock { $0 } }

    func report(_ issue: CriticalIssue) {
        issues.withLock { $0.append(issue) }
    }
}
