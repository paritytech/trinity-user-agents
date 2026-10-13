import Foundation
import IssueMonitoring

public struct NotificationSlotIssueDiagnostics: Sendable {
    let unsupportedRuntime: IssueDiagnostic
    let claimGaveUp: IssueDiagnostic
    let claimFailing: IssueDiagnostic
    let claimUnbuildable: IssueDiagnostic
}

public extension NotificationSlotIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(
                unsupportedRuntime: NoopIssueDiagnostic(),
                claimGaveUp: NoopIssueDiagnostic(),
                claimFailing: NoopIssueDiagnostic(),
                claimUnbuildable: NoopIssueDiagnostic()
            )
        }

        let flow: StaticString = "notification-slot"

        return Self(
            unsupportedRuntime: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "notification-slots-unsupported",
                reporter: reporter
            ),
            claimGaveUp: ThresholdIssueDiagnostic(flow: flow, kind: "slot-claim-gave-up", reporter: reporter),
            claimFailing: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "slot-claim-failing",
                threshold: 3,
                reporter: reporter
            ),
            claimUnbuildable: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "slot-claim-unbuildable",
                threshold: 10,
                reporter: reporter
            )
        )
    }
}
