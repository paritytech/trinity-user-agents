import IssueMonitoring

/// What the host reports when a product's chain connections or worker will not come up, through the
/// same threshold diagnostics as every other critical flow. Shared, so counts span every execution.
enum HostLoadIssueDiagnostics {
    private static let flow: StaticString = "productLoad"

    /// A chain whose connections keep dying before one is up; one dead node is routine, as the next dial
    /// moves to another.
    static let rpcConnection: IssueDiagnostic = make(kind: "rpcConnectionFailing", threshold: 3)

    /// A product worker that failed to boot.
    static let workerBoot: IssueDiagnostic = make(kind: "workerBootFailed", threshold: 1)

    private static func make(kind: StaticString, threshold: Int) -> IssueDiagnostic {
        guard IssueMonitoringFactory.isReportingSupported else { return NoopIssueDiagnostic() }

        return ThresholdIssueDiagnostic(
            flow: flow,
            kind: kind,
            threshold: threshold,
            reporter: IssueReportingFacade.shared
        )
    }
}
