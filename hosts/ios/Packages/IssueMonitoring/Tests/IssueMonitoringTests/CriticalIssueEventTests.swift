import Foundation
import IssueMonitoring
import Testing

struct CriticalIssueEventTests {
    private let origin = HostEventOrigin(
        launchID: UUID(),
        osVersion: "26.5",
        appVersion: "1.2.3",
        appBuild: 77,
        appVariant: "nightly"
    )

    @Test func anIssueGroupsOnItsFlowAndKindRatherThanOnTheNameTheySpell() {
        let event = CriticalIssue(flow: "productLoad", kind: "contentUnavailable")
            .hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event.name == "productLoad.contentUnavailable")
        #expect(event.fingerprint == ["productLoad", "contentUnavailable"])
    }

    @Test func anIssueIsLoudEnoughToTellApartFromALifecycleEvent() {
        let issue = CriticalIssue(flow: "productLoad", kind: "contentUnavailable")
        let lifecycle = HostEvent.make(name: "host.process.started", origin: origin, monotonicMilliseconds: 0)

        #expect(issue.hostEvent(origin: origin, monotonicMilliseconds: 0).level == .error)
        #expect(lifecycle.level == .info)
    }

    @Test func anIssueCarriesTheOriginFieldsEveryHostEventCarries() {
        let event = CriticalIssue(flow: "productLoad", kind: "contentUnavailable")
            .hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event.tags["host.platform"] == "ios")
        #expect(event.tags["host.app_variant"] == "nightly")
        #expect(event.tags["host.native_app_version"] == "1.2.3")
        #expect(event.extras["host.app_build"] == .integer(77))
    }

    @Test func anIssueKeepsItsOwnTagsAndItsCounters() throws {
        let event = CriticalIssue(
            flow: "productLoad",
            kind: "contentFetchStalled",
            error: StalledError(),
            counters: ["attempts": 3]
        ).hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event.tags["flow"] == "productLoad")
        #expect(event.tags["kind"] == "contentFetchStalled")
        #expect(event.tags["errorCode"] == "stalled")
        #expect(try #require(event.tags["errorType"]).hasSuffix("StalledError"))
        #expect(event.counters == ["attempts": 3])
    }

    @Test func anIssueWithoutAnErrorNamesNeitherItsTypeNorItsCode() {
        let event = CriticalIssue(flow: "chat-request", kind: "renewal-starved")
            .hostEvent(origin: origin, monotonicMilliseconds: 42)

        #expect(event.tags["errorType"] == nil)
        #expect(event.tags["errorCode"] == nil)
    }

    @Test func aLifecycleEventGroupsOnItsNameAloneAndCountsNothing() {
        let event = HostEvent.make(name: "host.process.started", origin: origin, monotonicMilliseconds: 42)

        #expect(event.fingerprint == ["host.process.started"])
        #expect(event.counters.isEmpty)
    }

    @Test func theReporterHandsTheIssueEventToTheRecorderTheHostAlreadyHas() {
        let recorder = RecordingHostEventRecorder()
        let reporter = HostEventIssueReporter(recorder: recorder, origin: origin, monotonicMilliseconds: { 7 })

        reporter.report(CriticalIssue(flow: "chat-request", kind: "renewal-starved"))

        #expect(recorder.events.count == 1)
        #expect(recorder.events.first?.fingerprint == ["chat-request", "renewal-starved"])
        #expect(recorder.events.first?.extras["event.monotonic_ms"] == .integer(7))
    }
}

/// Collects what a reporter hands over, standing in for the sender a build
/// without reporting support does not link.
private final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private(set) var events: [HostEvent] = []

    func record(_ event: @autoclosure () -> HostEvent) {
        events.append(event())
    }
}

private struct StalledError: ReportableError {
    let reportCode = "stalled"
}
