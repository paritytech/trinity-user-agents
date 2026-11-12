import Foundation
import os

/// Watches one critical case and reports it once its failures for a key reach a threshold. `key` stays on the
/// device.
public protocol IssueDiagnostic: Sendable {
    func recordFailure(for key: String, error: Error?, counters: [String: Int])
    func recordRecovery(for key: String)
}

public final class ThresholdIssueDiagnostic: IssueDiagnostic {
    private let flow: StaticString
    private let kind: StaticString
    private let threshold: Int
    private let reporter: IssueReporting
    private let failuresByKey = OSAllocatedUnfairLock<[String: Int]>(initialState: [:])

    public init(flow: StaticString, kind: StaticString, threshold: Int = 1, reporter: IssueReporting) {
        self.flow = flow
        self.kind = kind
        self.threshold = threshold
        self.reporter = reporter
    }

    public func recordFailure(for key: String, error: Error?, counters: [String: Int]) {
        let failures = failuresByKey.withLock { counts in
            counts[key, default: 0] += 1
            return counts[key, default: 0]
        }

        guard failures == threshold else { return }

        var reportedCounters = counters
        reportedCounters["failures"] = failures

        reporter.report(CriticalIssue(flow: flow, kind: kind, error: error, counters: reportedCounters))
    }

    public func recordRecovery(for key: String) {
        failuresByKey.withLock { _ = $0.removeValue(forKey: key) }
    }
}

public struct NoopIssueDiagnostic: IssueDiagnostic {
    public init() {}

    public func recordFailure(for _: String, error _: Error?, counters _: [String: Int]) {}

    public func recordRecovery(for _: String) {}
}
