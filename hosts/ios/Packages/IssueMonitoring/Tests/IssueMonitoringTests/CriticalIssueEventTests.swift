import Foundation
import Testing
@testable import IssueMonitoring

struct CriticalIssueEventTests {
    private let origin = HostEventOrigin(
        launchID: UUID(),
        osVersion: "26.5",
        appVersion: "1.2.3",
        appBuild: 77,
        appVariant: "nightly"
    )

    /// Asserting the whole event pins what an issue does not carry as well as
    /// what it does. A tag that arrives carrying free text fails this, which is
    /// what `CriticalIssue` taking its flow and kind as `StaticString` is for,
    /// and so does a grouping key that stops being the flow and the kind.
    @Test func anIssueBecomesExactlyThisEvent() throws {
        let event = CriticalIssue(
            flow: "productLoad",
            kind: "contentFetchStalled",
            error: StalledError(),
            counters: ["attempts": 3]
        ).hostEvent(origin: origin, monotonicMilliseconds: 42)

        // The type is named by its module, which keeps two same named errors in
        // different packages apart.
        let errorType = try #require(event.tags["errorType"])
        #expect(errorType.hasSuffix(".StalledError"))

        #expect(event == HostEvent(
            name: "productLoad.contentFetchStalled",
            level: .error,
            fingerprint: ["productLoad", "contentFetchStalled"],
            tags: [
                "flow": "productLoad",
                "kind": "contentFetchStalled",
                "errorType": errorType,
                "errorCode": "stalled",
                "host.os_version": "26.5",
                "host.app_variant": "nightly",
                "host.platform": "ios",
                "host.native_app_version": "1.2.3"
            ],
            extras: [
                "app.launch_id": .string(origin.launchID.uuidString.lowercased()),
                "host.app_build": .integer(77),
                "event.monotonic_ms": .integer(42)
            ],
            counters: ["attempts": 3]
        ))
    }

    @Test func anIssueWithoutAnErrorNamesNeitherItsTypeNorItsCode() {
        let event = CriticalIssue(flow: "chat-request", kind: "renewal-starved")
            .hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event.tags["errorType"] == nil)
        #expect(event.tags["errorCode"] == nil)
    }
}

private struct StalledError: ReportableError {
    let reportCode = "stalled"
}
