import Foundation

/// Reports the steps of a product's game reminder through the issue-monitoring abstraction.
public struct GameReminderDiagnostics: Sendable {
    private let reporter: IssueReporting

    public init(reporter: IssueReporting) {
        self.reporter = reporter
    }

    public func record(_ event: GameReminderEvent) {
        reporter.report(event)
    }
}

public extension GameReminderDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(reporter: NoopIssueReporter())
        }

        return Self(reporter: reporter)
    }
}
