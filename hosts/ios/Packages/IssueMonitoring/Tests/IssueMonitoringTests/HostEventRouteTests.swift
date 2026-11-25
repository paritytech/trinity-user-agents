import Foundation
import Testing
@testable import IssueMonitoring

struct HostEventRouteTests {
    @Test func anInfoEventLeavesAsALogLineCarryingItsTagsAndExtras() {
        let event = HostEvent(
            name: "host.test.stepped",
            level: .info,
            fingerprint: ["host.test", "stepped"],
            tags: ["flow": "test", "shared": "tag"],
            extras: ["attempt": .integer(2), "shared": .boolean(true)],
            counters: ["skipped": 1]
        )

        #expect(event.route == .log(
            body: "host.test.stepped",
            attributes: [
                "flow": .string("test"),
                "attempt": .integer(2),
                "shared": .boolean(true),
                "tag": .string("host.telemetry"),
                "event.name": .string("host.test.stepped")
            ]
        ))
    }

    @Test func anEventCannotOverwriteTheAttributesALogQueryScopesOn() {
        let event = HostEvent(
            name: "host.test.stepped",
            tags: ["tag": "truapi.core"],
            extras: ["event.name": .string("other")]
        )

        guard case let .log(_, attributes) = event.route else {
            Issue.record("an info event is a log line")
            return
        }

        #expect(attributes["tag"] == .string("host.telemetry"))
        #expect(attributes["event.name"] == .string("host.test.stepped"))
    }

    @Test(arguments: [HostLogLevel.warning, .error])
    func anEventAboveInfoIsCaptured(level: HostLogLevel) {
        let event = HostEvent(name: "host.test.broke", level: level, tags: [:], extras: [:])

        #expect(event.route == .capture)
    }
}
