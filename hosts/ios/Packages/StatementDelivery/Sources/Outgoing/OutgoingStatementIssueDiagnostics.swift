import Foundation
import IssueMonitoring
import SDKLogger

public struct OutgoingStatementIssueDiagnostics: Sendable {
    public let oversized: IssueDiagnostic
    public let deliveryStalled: IssueDiagnostic
    public let deliveryStateUnreadable: IssueDiagnostic
    public let renewalPublishFailed: IssueDiagnostic
    public let renewalFailing: IssueDiagnostic

    public init(
        oversized: IssueDiagnostic,
        deliveryStalled: IssueDiagnostic,
        deliveryStateUnreadable: IssueDiagnostic,
        renewalPublishFailed: IssueDiagnostic,
        renewalFailing: IssueDiagnostic
    ) {
        self.oversized = oversized
        self.deliveryStalled = deliveryStalled
        self.deliveryStateUnreadable = deliveryStateUnreadable
        self.renewalPublishFailed = renewalPublishFailed
        self.renewalFailing = renewalFailing
    }
}

public struct OutgoingStatementDiagnostics: Sendable {
    public let logger: SDKLoggerProtocol
    public let issues: OutgoingStatementIssueDiagnostics

    public init(logger: SDKLoggerProtocol, issues: OutgoingStatementIssueDiagnostics) {
        self.logger = logger
        self.issues = issues
    }
}
