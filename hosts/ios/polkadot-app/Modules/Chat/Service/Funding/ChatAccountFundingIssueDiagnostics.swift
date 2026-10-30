import IssueMonitoring

struct ChatAccountFundingIssueDiagnostics: Sendable {
    let fundingFailing: IssueDiagnostic
}

extension ChatAccountFundingIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(fundingFailing: NoopIssueDiagnostic())
        }

        return Self(
            fundingFailing: ThresholdIssueDiagnostic(
                flow: "chat-account-funding",
                kind: "funding-failing",
                threshold: 3,
                reporter: reporter
            )
        )
    }
}
