import Foundation
import IssueMonitoring
import os

final class RecordingIssueReporter: IssueReporting, @unchecked Sendable {
    private let reportedIssues = OSAllocatedUnfairLock(initialState: [CriticalIssue]())

    var issues: [CriticalIssue] {
        reportedIssues.withLock { $0 }
    }

    func report(_ issue: CriticalIssue) {
        reportedIssues.withLock { $0.append(issue) }
    }
}
