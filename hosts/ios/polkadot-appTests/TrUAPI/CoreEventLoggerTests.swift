import IssueMonitoring
import Testing
import TrUAPIHost
@testable import polkadot_app

struct CoreEventLoggerTests {
    private let attributes = [
        CoreAttribute(key: "outcome", value: .text(value: "sent")),
        CoreAttribute(key: "age_ms", value: .count(value: 1_200)),
        CoreAttribute(key: "money_moved", value: .flag(value: true))
    ]

    @Test func anExportedMarkerReachesTheSinkWithItsAttributes() {
        let logger = RecordingLogger()
        let sink = RecordingHostLogSink()
        let coreEventLogger = CoreEventLogger(logger: logger, sink: sink)

        coreEventLogger.log(marker: "truapi.chain.follow_budget_reached", attributes: attributes)

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
        let coreEventLogger = CoreEventLogger(logger: logger, sink: sink)

        coreEventLogger.log(marker: "truapi.native.callback.push_notification", attributes: attributes)

        #expect(sink.lines.isEmpty)
        #expect(logger.lines.count == 1)
    }

    @Test func aMarkerTheTableDoesNotListStaysOnTheDevice() {
        let logger = RecordingLogger()
        let sink = RecordingHostLogSink()
        let coreEventLogger = CoreEventLogger(logger: logger, sink: sink)

        coreEventLogger.log(marker: "truapi.chain.not_registered", attributes: attributes)

        #expect(sink.lines.isEmpty)
        #expect(logger.lines.count == 1)
    }
}
