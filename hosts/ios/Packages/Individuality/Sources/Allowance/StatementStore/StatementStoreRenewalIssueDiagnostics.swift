import Foundation
import IssueMonitoring

public struct StatementStoreRenewalIssueDiagnostics: Sendable {
    let renewalFailing: IssueDiagnostic
    let slotRejected: IssueDiagnostic
    let slotRenewalFailing: IssueDiagnostic
    let overflowDropped: IssueDiagnostic
}

public extension StatementStoreRenewalIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(
                renewalFailing: NoopIssueDiagnostic(),
                slotRejected: NoopIssueDiagnostic(),
                slotRenewalFailing: NoopIssueDiagnostic(),
                overflowDropped: NoopIssueDiagnostic()
            )
        }

        let flow: StaticString = "statement-store-renewal"

        return Self(
            renewalFailing: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "renewal-failing",
                threshold: 3,
                reporter: reporter
            ),
            slotRejected: ThresholdIssueDiagnostic(flow: flow, kind: "slot-rejected", reporter: reporter),
            slotRenewalFailing: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "slot-renewal-failing",
                threshold: 3,
                reporter: reporter
            ),
            overflowDropped: ThresholdIssueDiagnostic(flow: flow, kind: "overflow-dropped", reporter: reporter)
        )
    }
}
