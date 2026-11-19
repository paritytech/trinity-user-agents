import Foundation
import IssueMonitoring
import SwiftyBeaver
import Testing
import TrUAPIHost
@testable import polkadot_app

struct CoreEventLoggerTests {
    @Test func anExportedMarkerIsLoggedAtItsLevelWithItsAttributes() throws {
        let logger = RecordingCoreEventLogger()

        logger.logCoreEvent(marker: "truapi.chain.follow_budget_reached", attributes: attributes(outcome: "sent"))

        let event = try #require(logger.events.first)
        #expect(logger.events.count == 1)
        #expect(event.level == .warning)
        #expect(event.message == [
            "[truapi.core] truapi.chain.follow_budget_reached:",
            "outcome=sent age_ms=1200 money_moved=true"
        ].joined(separator: " "))
        #expect(event.context.marker == "truapi.chain.follow_budget_reached")
        #expect(event.context.attributes == expectedAttributes(outcome: "sent"))
    }

    @Test func aLocalOrUnlistedMarkerIsLoggedAtDebug() {
        let logger = RecordingCoreEventLogger()

        logger.logCoreEvent(marker: "truapi.native.callback.push_notification", attributes: attributes(outcome: "sent"))
        logger.logCoreEvent(marker: "truapi.chain.not_registered", attributes: attributes(outcome: "sent"))

        #expect(logger.events.map(\.level) == [nil, nil])
    }

    @Test func theLastDuplicateKeyWins() throws {
        let logger = RecordingCoreEventLogger()

        logger.logCoreEvent(marker: "truapi.chain.follow_budget_reached", attributes: [
            CoreAttribute(key: "outcome", value: .text(value: "first")),
            CoreAttribute(key: "outcome", value: .text(value: "last"))
        ])

        let event = try #require(logger.events.first)
        #expect(event.context.attributes == ["outcome": .text("last")])
    }

    /// SwiftyBeaver applies the level threshold before `send`, so the events go through SwiftyBeaver itself.
    @Test func anExportedMarkerReachesTheSinkWithItsAttributes() {
        let sink = RecordingHostLogSink()
        let token = UUID().uuidString

        withSinkDestination(sink) {
            Logger.shared.logCoreEvent(
                marker: "truapi.chain.follow_budget_reached",
                attributes: attributes(outcome: token)
            )
        }

        #expect(sink.lines.filter { $0.attributes["outcome"] == .text(token) } == [
            RecordingHostLogSink.Line(
                level: .warning,
                tag: "truapi.core",
                message: "truapi.chain.follow_budget_reached",
                attributes: expectedAttributes(outcome: token)
            )
        ])
    }

    @Test func aLocalOrUnlistedMarkerNeverReachesTheSink() {
        let sink = RecordingHostLogSink()
        let token = UUID().uuidString

        withSinkDestination(sink) {
            Logger.shared.logCoreEvent(
                marker: "truapi.native.callback.push_notification",
                attributes: attributes(outcome: token)
            )
            Logger.shared.logCoreEvent(marker: "truapi.chain.not_registered", attributes: attributes(outcome: token))
        }

        #expect(!sink.lines.contains { $0.attributes["outcome"] == .text(token) })
    }
}

private extension CoreEventLoggerTests {
    func attributes(outcome: String) -> [CoreAttribute] {
        [
            CoreAttribute(key: "outcome", value: .text(value: outcome)),
            CoreAttribute(key: "age_ms", value: .count(value: 1_200)),
            CoreAttribute(key: "money_moved", value: .flag(value: true))
        ]
    }

    func expectedAttributes(outcome: String) -> [String: HostLogAttribute] {
        ["outcome": .text(outcome), "age_ms": .count(1_200), "money_moved": .flag(true)]
    }

    func withSinkDestination(_ sink: HostLogSink, _ body: () -> Void) {
        let destination = HostLogSinkDestination(sink: sink)
        destination.asynchronously = false
        SwiftyBeaver.addDestination(destination)
        defer { SwiftyBeaver.removeDestination(destination) }

        body()
    }
}

private final class RecordingCoreEventLogger: CoreEventLoggerProtocol, @unchecked Sendable {
    struct Event {
        let level: HostLogLevel?
        let message: String
        let context: CoreEventContext
    }

    private let lock = NSLock()
    private var recorded: [Event] = []

    var events: [Event] {
        lock.withLock { recorded }
    }

    func log(level: HostLogLevel?, message: String, context: CoreEventContext) {
        lock.withLock { recorded.append(Event(level: level, message: message, context: context)) }
    }

    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}
