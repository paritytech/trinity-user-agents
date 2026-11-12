import Foundation
import IssueMonitoring

public struct AllowanceIssueDiagnostics: Sendable {
    let allocationFailing: IssueDiagnostic
    let slotsExhausted: IssueDiagnostic
}

public extension AllowanceIssueDiagnostics {
    static func make(flow: StaticString, reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(allocationFailing: NoopIssueDiagnostic(), slotsExhausted: NoopIssueDiagnostic())
        }

        return Self(
            allocationFailing: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "allocation-failing",
                threshold: 3,
                reporter: reporter
            ),
            slotsExhausted: ThresholdIssueDiagnostic(flow: flow, kind: "slots-exhausted", reporter: reporter)
        )
    }
}

extension AllowanceIssueDiagnostics {
    private static let allocationKey = "allocation"

    func observeAllocation<T>(_ allocation: () async throws -> T) async throws -> T {
        do {
            let result = try await allocation()
            allocationFailing.recordRecovery(for: Self.allocationKey)
            slotsExhausted.recordRecovery(for: Self.allocationKey)
            return result
        } catch is CancellationError {
            throw CancellationError()
        } catch AllowanceSlotAssignmentError.noSlotsAvailable {
            slotsExhausted.recordFailure(for: Self.allocationKey, error: nil, counters: [:])
            throw AllowanceSlotAssignmentError.noSlotsAvailable
        } catch {
            allocationFailing.recordFailure(for: Self.allocationKey, error: error, counters: [:])
            throw error
        }
    }
}
