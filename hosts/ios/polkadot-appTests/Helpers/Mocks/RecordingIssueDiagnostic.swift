import Foundation
import IssueMonitoring
import os

final class RecordingIssueDiagnostic: IssueDiagnostic, @unchecked Sendable {
    private struct State {
        var failures: [[String: Int]] = []
        var recoveries = 0
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var failureCount: Int { state.withLock(\.failures.count) }
    var recoveryCount: Int { state.withLock(\.recoveries) }
    var lastCounters: [String: Int]? { state.withLock(\.failures.last) }

    func recordFailure(for _: String, error _: Error?, counters: [String: Int]) {
        state.withLock { $0.failures.append(counters) }
    }

    func recordRecovery(for _: String) {
        state.withLock { $0.recoveries += 1 }
    }
}
