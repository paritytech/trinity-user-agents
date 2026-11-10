import IssueMonitoring
import StatementDelivery

struct ChatAcceptanceIssueDiagnostics: Sendable {
    let publishFailing: IssueDiagnostic
}

extension ChatAcceptanceIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(publishFailing: NoopIssueDiagnostic())
        }

        return Self(
            publishFailing: ThresholdIssueDiagnostic(
                flow: "chat-acceptance",
                kind: "publish-failing",
                threshold: 3,
                reporter: reporter
            )
        )
    }

    func statementDiagnostics(logger: LoggerProtocol) -> OutgoingStatementDiagnostics {
        OutgoingStatementDiagnostics(
            logger: logger,
            issues: OutgoingStatementIssueDiagnostics(
                oversized: publishFailing,
                deliveryStalled: publishFailing,
                deliveryStateUnreadable: NoopIssueDiagnostic(),
                renewalPublishFailed: publishFailing,
                renewalFailing: NoopIssueDiagnostic()
            )
        )
    }
}
