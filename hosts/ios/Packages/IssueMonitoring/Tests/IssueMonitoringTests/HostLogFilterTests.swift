import Foundation
import Testing
@testable import IssueMonitoring

struct HostLogFilterTests {
    @Test func aLogThisHostDidNotWriteIsTurnedAway() {
        let filter = HostLogFilter()

        #expect(filter.verdict(tag: nil, body: "a line from somewhere else", eventName: nil) == .drop)
        #expect(filter.verdict(tag: "DeviceSync", body: "peer lost", eventName: nil) == .drop)
        #expect(filter.verdict(tag: "", body: "peer lost", eventName: nil) == .drop)
    }

    @Test func aHostTagAndTheHostEventTagAreSent() {
        let filter = HostLogFilter()

        #expect(filter.verdict(tag: "truapi.chain", body: "connection 1 closing", eventName: nil)
            == .send(dropped: 0))
        #expect(filter.verdict(tag: HostEvent.logTag, body: "host.test.stepped", eventName: "host.test.stepped")
            == .send(dropped: 0))
    }

    @Test func aLoopingTagSendsABoundedNumberOfLinesAndThenCountsWhatItDropped() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })
        let limit = HostLogFilter.linesPerWindow

        var sent = 0
        for index in 0 ..< limit + 500 {
            if filter.verdict(tag: "truapi.chain", body: "follow p:\(index) restarted", eventName: nil)
                != .drop { sent += 1 }
        }

        #expect(sent == limit)
        #expect(filter.verdict(tag: "truapi.auth", body: "session restored", eventName: nil) == .send(dropped: 0))

        clock.now += HostLogFilter.window

        #expect(filter.verdict(tag: "truapi.chain", body: "follow p:9999 restarted", eventName: nil)
            == .send(dropped: 500))
        #expect(filter.verdict(tag: "truapi.chain", body: "follow p:10000 restarted", eventName: nil)
            == .send(dropped: 0))
    }

    @Test func aWindowThatDroppedNothingCountsNothing() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })

        #expect(filter.verdict(tag: "truapi.chain", body: "first", eventName: nil) == .send(dropped: 0))
        clock.now += HostLogFilter.window
        #expect(filter.verdict(tag: "truapi.chain", body: "second", eventName: nil) == .send(dropped: 0))
    }

    @Test func eachCoreMarkerHasItsOwnWindow() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })
        let limit = HostLogFilter.linesPerWindow

        for index in 0 ..< limit + 10 {
            _ = filter.verdict(tag: "truapi.core", body: "truapi.tracing.warn: chain call failed \(index)",
                               eventName: nil)
        }

        #expect(filter.verdict(tag: "truapi.core", body: "truapi.chain.follow_budget_reached: c1 open 4s",
                               eventName: nil) == .send(dropped: 0))

        clock.now += HostLogFilter.window

        #expect(filter.verdict(tag: "truapi.core", body: "truapi.tracing.warn: chain call failed again",
                               eventName: nil) == .send(dropped: 10))
    }

    /// A core event's body is its marker on its own, with no fields after it,
    /// so it counts against the same window as the core line of that marker.
    @Test func aCoreEventCountsAgainstItsOwnMarker() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })
        let limit = HostLogFilter.linesPerWindow

        for _ in 0 ..< limit {
            _ = filter.verdict(tag: "truapi.core", body: "truapi.funding.step_reached", eventName: nil)
        }

        #expect(filter.verdict(tag: "truapi.core", body: "truapi.funding.step_reached", eventName: nil) == .drop)
        #expect(filter.verdict(tag: "truapi.core", body: "truapi.funding.ended", eventName: nil)
            == .send(dropped: 0))
    }

    @Test func eachHostEventNameHasItsOwnWindow() {
        let clock = ManualClock()
        let filter = HostLogFilter(now: { clock.now })
        let limit = HostLogFilter.linesPerWindow

        for _ in 0 ..< limit {
            _ = filter.verdict(tag: HostEvent.logTag, body: "host.notification.not_scheduled",
                               eventName: "host.notification.not_scheduled")
        }

        #expect(filter.verdict(tag: HostEvent.logTag, body: "host.notification.not_scheduled",
                               eventName: "host.notification.not_scheduled") == .drop)
        #expect(filter.verdict(tag: HostEvent.logTag, body: "host.product.opened",
                               eventName: "host.product.opened") == .send(dropped: 0))
    }
}

private final class ManualClock: @unchecked Sendable {
    var now: TimeInterval = 0
}
