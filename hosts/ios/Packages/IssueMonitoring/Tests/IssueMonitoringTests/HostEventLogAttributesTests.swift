import Foundation
import Testing
@testable import IssueMonitoring

/// An info event leaves as a Sentry log, whose attributes are one namespace for tags, extras and counters.
struct HostEventLogAttributesTests {
    /// Asserting the whole set pins every key an attribute is sent under, so a field that stops arriving fails here.
    @Test func everyTagExtraAndCounterBecomesAnAttribute() {
        let event = HostEvent(
            name: "host.page.visible",
            tags: ["host.session_id": "d93fe846-8f3c-4cfa-a440-402b6f7b43d3", "open.source": "pocket_card"],
            extras: ["tap_ms": .integer(922), "webview_ms": .integer(781)],
            counters: ["retries": 2]
        )

        #expect(event.logAttributes == [
            "host.session_id": .string("d93fe846-8f3c-4cfa-a440-402b6f7b43d3"),
            "open.source": .string("pocket_card"),
            "tap_ms": .integer(922),
            "webview_ms": .integer(781),
            "counters.retries": .integer(2),
            "event.name": .string("host.page.visible"),
            "tag": .string("host.telemetry")
        ])
    }

    /// A tag is what a query selects on, so an extra under the same key never replaces it, and is kept beside it.
    @Test func anExtraNamedLikeATagKeepsTheTagAndArrivesBesideIt() {
        let event = HostEvent(
            name: "host.webview.created",
            tags: ["host.session_id": "from-the-tag"],
            extras: ["host.session_id": .string("from-the-extra")]
        )

        #expect(event.logAttributes["host.session_id"] == .string("from-the-tag"))
        #expect(event.logAttributes["extra.host.session_id"] == .string("from-the-extra"))
    }

    /// Counters ride on a context as an event, and on the log as attributes of their own, never dropped.
    @Test func countersArriveOnTheLog() {
        let event = HostEvent(name: "host.thing", tags: [:], extras: [:], counters: ["failures": 3, "attempts": 5])

        #expect(event.logAttributes["counters.failures"] == .integer(3))
        #expect(event.logAttributes["counters.attempts"] == .integer(5))
    }
}
