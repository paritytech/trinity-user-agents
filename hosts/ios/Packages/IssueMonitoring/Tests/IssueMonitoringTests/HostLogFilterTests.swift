import Foundation
import Testing
@testable import IssueMonitoring

struct HostLogFilterTests {
    @Test(arguments: [
        (String?.none, false),
        ("", false),
        ("DeviceSync", false),
        ("truapi.chain", true),
        (HostLogFilter.coreTag, true),
        (HostEvent.logTag, true)
    ] as [(String?, Bool)])
    func onlyATagThisHostWritesIsSent(tag: String?, sent: Bool) {
        let verdict = HostLogFilter().verdict(tag: tag, body: "connection 1 closing", eventName: nil)

        #expect(verdict == (sent ? .send(dropped: 0) : .drop))
    }

    /// What a line counts against: its marker on `truapi.core`, its name as a
    /// host event, and its tag where it names neither.
    @Test(arguments: [
        (
            HostLogLine.core("truapi.tracing.warn: chain call failed"),
            HostLogLine.core("truapi.tracing.warn: chain call failed again"),
            true
        ),
        (
            HostLogLine.core("truapi.funding.step_reached"),
            HostLogLine.core("truapi.funding.step_reached: step 3 of 15"),
            true
        ),
        (
            HostLogLine.core(tooLongToNameAMarker),
            HostLogLine.core(tooLongToNameAMarker + " and then some"),
            true
        ),
        (
            HostLogLine.core(": fields with no marker before them"),
            HostLogLine.core(tooLongToNameAMarker),
            true
        ),
        (
            HostLogLine.event("host.notification.not_scheduled"),
            HostLogLine.event("host.notification.not_scheduled"),
            true
        ),
        (HostLogLine.tagged("truapi.chain"), HostLogLine.tagged("truapi.chain"), true),
        (HostLogLine.tagged("truapi.chain"), HostLogLine.tagged("truapi.auth"), false),
        (
            HostLogLine.core("truapi.tracing.warn: chain call failed"),
            HostLogLine.core("truapi.chain.follow_budget_reached: c1 open 4s"),
            false
        ),
        (
            HostLogLine.core("truapi.funding.step_reached"),
            HostLogLine.core("truapi.funding.ended"),
            false
        ),
        (
            HostLogLine.core(tooLongToNameAMarker),
            HostLogLine.core("truapi.funding.ended"),
            false
        ),
        (
            HostLogLine.event("host.notification.not_scheduled"),
            HostLogLine.event("host.product.opened"),
            false
        )
    ])
    func twoLinesShareAWindowOnlyWhenTheyShareAKey(first: HostLogLine, second: HostLogLine, shared: Bool) {
        let filter = HostLogFilter()

        for _ in 0 ..< HostLogFilter.linesPerWindow {
            _ = filter.verdict(tag: first.tag, body: first.body, eventName: first.eventName)
        }

        let verdict = filter.verdict(tag: second.tag, body: second.body, eventName: second.eventName)

        #expect(verdict == (shared ? .drop : .send(dropped: 0)))
    }

    @Test func aLoopingKeySendsABoundedNumberOfLinesThenCountsWhatItDropped() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })
        let limit = HostLogFilter.linesPerWindow

        let sent = (0 ..< limit + 500).reduce(into: 0) { total, index in
            if filter.verdict(tag: "truapi.chain", body: "follow p:\(index) restarted", eventName: nil) != .drop {
                total += 1
            }
        }

        #expect(sent == limit)

        clock.now += HostLogFilter.window

        #expect(filter.verdict(tag: "truapi.chain", body: "follow p:9999 restarted", eventName: nil)
            == .send(dropped: 500))

        clock.now += HostLogFilter.window

        #expect(filter.verdict(tag: "truapi.chain", body: "follow p:10000 restarted", eventName: nil)
            == .send(dropped: 0))
    }

    /// The gate is fail closed, so a producer writing its tag under any other
    /// key would have every one of its logs turned away.
    @Test func aHostEventIsWrittenUnderTheKeysTheGateReads() {
        let event = HostEvent(
            name: "host.test.stepped",
            level: .info,
            fingerprint: ["host.test", "stepped"],
            tags: [:],
            extras: [:],
            counters: [:]
        )

        guard case let .log(_, attributes) = event.route else {
            Issue.record("an info host event did not leave as a log line")
            return
        }

        #expect(attributes[HostLogFilter.tagAttribute] == .string(HostEvent.logTag))
        #expect(attributes[HostLogFilter.eventNameAttribute] == .string("host.test.stepped"))
    }
}

/// A body long enough that the gate reads no marker out of it.
private let tooLongToNameAMarker = String(repeating: "x", count: HostLogFilter.markerLimit)

struct HostLogLine: Sendable {
    var tag: String
    var body: String
    var eventName: String?

    static func core(_ body: String) -> HostLogLine {
        HostLogLine(tag: HostLogFilter.coreTag, body: body, eventName: nil)
    }

    static func event(_ name: String) -> HostLogLine {
        HostLogLine(tag: HostEvent.logTag, body: name, eventName: name)
    }

    static func tagged(_ tag: String) -> HostLogLine {
        HostLogLine(tag: tag, body: "connection 1 closing", eventName: nil)
    }
}

private final class ManualClock: @unchecked Sendable {
    var now: TimeInterval = 0
}
