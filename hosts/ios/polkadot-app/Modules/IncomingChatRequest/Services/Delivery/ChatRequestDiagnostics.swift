import IssueMonitoring

struct ChatRequestDiagnostics {
    let logger: LoggerProtocol
    let issues: ChatRequestIssueDiagnostics
}

struct ChatRequestIssueDiagnostics: Sendable {
    let oversizedRequest: IssueDiagnostic
    let deliveryStalled: IssueDiagnostic
    let deliveryStateUnreadable: IssueDiagnostic
    let renewalPublishFailed: IssueDiagnostic
    let renewalStarved: IssueDiagnostic
    let renewalFailing: IssueDiagnostic
}

extension ChatRequestIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(
                oversizedRequest: NoopIssueDiagnostic(),
                deliveryStalled: NoopIssueDiagnostic(),
                deliveryStateUnreadable: NoopIssueDiagnostic(),
                renewalPublishFailed: NoopIssueDiagnostic(),
                renewalStarved: NoopIssueDiagnostic(),
                renewalFailing: NoopIssueDiagnostic()
            )
        }

        let flow: StaticString = "chat-request"

        return Self(
            oversizedRequest: ThresholdIssueDiagnostic(flow: flow, kind: "oversized-request", reporter: reporter),
            deliveryStalled: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "delivery-stalled",
                threshold: 15,
                reporter: reporter
            ),
            deliveryStateUnreadable: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "delivery-state-unreadable",
                reporter: reporter
            ),
            renewalPublishFailed: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "renewal-publish-failed",
                reporter: reporter
            ),
            renewalStarved: ThresholdIssueDiagnostic(flow: flow, kind: "renewal-starved", reporter: reporter),
            renewalFailing: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "renewal-failing",
                threshold: 6,
                reporter: reporter
            )
        )
    }
}
