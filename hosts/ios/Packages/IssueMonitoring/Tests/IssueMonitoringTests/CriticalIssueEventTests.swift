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

    /// Asserting the whole event pins what an issue does not carry as well as what
    /// it does, which is the point for a type whose flow and kind are the only free
    /// text it is allowed.
    @Test func anIssueBecomesExactlyThisEvent() {
        let event = CriticalIssue(flow: "productLoad", kind: "contentUnavailable")
            .hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event == HostEvent(
            name: "productLoad.contentUnavailable",
            level: .error,
            fingerprint: ["productLoad", "contentUnavailable"],
            tags: [
                "flow": "productLoad",
                "kind": "contentUnavailable",
                "host.os_version": "26.5",
                "host.app_variant": "nightly",
                "host.platform": "ios",
                "host.native_app_version": "1.2.3"
            ],
            extras: [
                "app.launch_id": .string(origin.launchID.uuidString.lowercased()),
                "host.app_build": .integer(77),
                "event.monotonic_ms": .integer(42)
            ]
        ))
    }

    @Test func anIssueWithAnErrorNamesItsTypeItsCodeAndItsCounters() throws {
        let event = CriticalIssue(
            flow: "productLoad",
            kind: "contentFetchStalled",
            error: StalledError(),
            counters: ["attempts": 3]
        ).hostEvent(origin: origin, monotonicMilliseconds: 42)

        // The leading dot is the assertion: the type is named by its module, which
        // is what keeps two same named errors in different packages apart.
        #expect(try #require(event.tags["errorType"]).hasSuffix(".StalledError"))
        #expect(event.tags["errorCode"] == "stalled")
        #expect(event.counters == ["attempts": 3])
    }

    @Test func aLifecycleEventGroupsOnItsNameAloneAndCountsNothing() {
        let event = HostEvent.make(name: "host.process.started", origin: origin, monotonicMilliseconds: 42)

        #expect(event.level == .info)
        #expect(event.fingerprint == ["host.process.started"])
        #expect(event.counters.isEmpty)
    }

    @Test func anEmptyGroupingKeyFallsBackToTheNameRatherThanReachingTheBackendEmpty() {
        let event = HostEvent(name: "host.test", fingerprint: [], tags: [:], extras: [:])

        #expect(event.fingerprint == ["host.test"])
    }

    @Test func theReporterReadsTheClockOnceAnIssueRatherThanOnceASession() {
        let recorder = RecordingHostEventRecorder()
        let readings = Readings()
        let reporter = HostEventIssueReporter(
            recorder: recorder,
            origin: origin,
            monotonicMilliseconds: { readings.next() }
        )

        reporter.report(CriticalIssue(flow: "chat-request", kind: "renewal-starved"))
        reporter.report(CriticalIssue(flow: "chat-request", kind: "renewal-starved"))

        #expect(recorder.events.count == 2)
        #expect(recorder.events.map { $0.extras["event.monotonic_ms"] } == [.integer(1), .integer(2)])
    }
}

/// Collects what a reporter hands over, standing in for the sender a build
/// without reporting support does not link.
private final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [HostEvent] = []

    var events: [HostEvent] {
        lock.withLock { recorded }
    }

    func record(_ event: @autoclosure () -> HostEvent) {
        let event = event()
        lock.withLock { recorded.append(event) }
    }
}

/// A clock that moves, so a reporter caching its first reading fails.
private final class Readings: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    func next() -> Int {
        lock.withLock {
            count += 1
            return count
        }
    }
}

struct StalledError: ReportableError {
    let reportCode = "stalled"
}
