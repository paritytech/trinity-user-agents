import Foundation
import IssueMonitoring

final class RecordingIssueReporter: IssueReporting, @unchecked Sendable {
    private let lock = NSLock()
    private var reportedIssues: [CriticalIssue] = []

    var issues: [CriticalIssue] {
        lock.withLock { reportedIssues }
    }

    func report(_ issue: CriticalIssue) {
        lock.withLock { reportedIssues.append(issue) }
    }
}
