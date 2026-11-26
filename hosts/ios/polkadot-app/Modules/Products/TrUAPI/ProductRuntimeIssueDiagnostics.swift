import IssueMonitoring

/// What the host reports when a product's chain connections or worker will not come up, through the
/// same threshold diagnostics as every other critical flow.
///
/// Their own flow, `productRuntime`: `productLoad` is `ProductLoadDiagnostics`' fetching of a product's
/// content, which neither of these touches.
///
/// The composition point builds one and hands it to the runtime provider, so its counts span every
/// execution the provider opens.
struct ProductRuntimeIssueDiagnostics: Sendable {
    /// A chain whose connections keep dying before one is up; one dead node is routine, as the next dial
    /// moves to another.
    let rpcConnection: IssueDiagnostic
    /// A product worker that keeps failing to boot; one failed boot is routine, as the next demand retries.
    let workerBoot: IssueDiagnostic
}

extension ProductRuntimeIssueDiagnostics {
    static func make(reporter: IssueReporting) -> Self {
        guard IssueMonitoringFactory.isReportingSupported else {
            return Self(rpcConnection: NoopIssueDiagnostic(), workerBoot: NoopIssueDiagnostic())
        }

        let flow: StaticString = "productRuntime"

        return Self(
            rpcConnection: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "rpcConnectionFailing",
                threshold: 3,
                reporter: reporter
            ),
            workerBoot: ThresholdIssueDiagnostic(
                flow: flow,
                kind: "workerBootFailed",
                threshold: 3,
                reporter: reporter
            )
        )
    }

    static let none = Self(rpcConnection: NoopIssueDiagnostic(), workerBoot: NoopIssueDiagnostic())
}
