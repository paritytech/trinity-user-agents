import IssueMonitoring
import Testing
import TrUAPIHost
@testable import polkadot_app

struct CoreEventTests {
    private let attributes = [
        CoreAttribute(key: "outcome", value: .text(value: "sent")),
        CoreAttribute(key: "age_ms", value: .count(value: 1_200)),
        CoreAttribute(key: "money_moved", value: .flag(value: true))
    ]

    @Test func anExportedMarkerReachesTheSinkWithItsAttributes() {
        let logger = RecordingLogger()
        let sink = RecordingHostLogSink()

        logger.logCoreEvent(marker: "truapi.chain.follow_budget_reached", attributes: attributes, sink: sink)

        #expect(sink.lines == [
            RecordingHostLogSink.Line(
                level: .warning,
                tag: "truapi.core",
                message: "truapi.chain.follow_budget_reached",
                attributes: ["outcome": .text("sent"), "age_ms": .count(1_200), "money_moved": .flag(true)]
            )
        ])
        #expect(logger.lines == [
            .debug("[truapi.core] truapi.chain.follow_budget_reached: outcome=sent age_ms=1200 money_moved=true")
        ])
    }

    @Test func aLocalMarkerStaysOnTheDevice() {
        let logger = RecordingLogger()
        let sink = RecordingHostLogSink()

        logger.logCoreEvent(marker: "truapi.native.callback.push_notification", attributes: attributes, sink: sink)

        #expect(sink.lines.isEmpty)
        #expect(logger.lines.count == 1)
    }

    @Test func aMarkerTheTableDoesNotListStaysOnTheDevice() {
        let logger = RecordingLogger()
        let sink = RecordingHostLogSink()

        logger.logCoreEvent(marker: "truapi.chain.not_registered", attributes: attributes, sink: sink)

        #expect(sink.lines.isEmpty)
        #expect(logger.lines.count == 1)
    }
}
