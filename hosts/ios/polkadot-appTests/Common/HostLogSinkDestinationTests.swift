import Foundation
import IssueMonitoring
import SwiftyBeaver
import Testing
@testable import polkadot_app

struct HostLogSinkDestinationTests {
    @Test func onlyTruapiLinesReachTheSinkWithTheirTag() {
        let sink = RecordingHostLogSink()
        let destination = HostLogSinkDestination(sink: sink)

        send(.info, "[truapi.chain] chain connection 1 dialling wss://node.example", to: destination)
        send(.warning, "[truapi.chain] chain connection 1 closing", to: destination)
        send(.error, "[truapi.chain] chain connection 1 failed", to: destination)
        send(.warning, "[DeviceSync] peer lost", to: destination)
        send(.error, "App Version: 1.0", to: destination)

        #expect(sink.lines == [
            RecordingHostLogSink.Line(
                level: .info,
                tag: "truapi.chain",
                message: "chain connection 1 dialling wss://node.example"
            ),
            RecordingHostLogSink.Line(level: .warning, tag: "truapi.chain", message: "chain connection 1 closing"),
            RecordingHostLogSink.Line(level: .error, tag: "truapi.chain", message: "chain connection 1 failed")
        ])
    }

    /// SwiftyBeaver applies the level threshold before `send`, so the lines go through SwiftyBeaver itself.
    @Test func debugLinesStayOnTheDevice() {
        let sink = RecordingHostLogSink()
        let destination = HostLogSinkDestination(sink: sink)
        destination.asynchronously = false
        SwiftyBeaver.addDestination(destination)
        defer { SwiftyBeaver.removeDestination(destination) }
        let token = UUID().uuidString

        SwiftyBeaver.debug("[truapi.chain] debug \(token)")
        SwiftyBeaver.info("[truapi.chain] info \(token)")

        #expect(sink.lines.filter { $0.message.hasSuffix(token) } == [
            RecordingHostLogSink.Line(level: .info, tag: "truapi.chain", message: "info \(token)")
        ])
    }

    @Test func aLoopingTagSendsABoundedNumberOfLinesAndThenCountsWhatItDropped() {
        let sink = RecordingHostLogSink()
        let clock = ManualClock()
        let destination = HostLogSinkDestination(sink: sink, now: { clock.now })
        let limit = HostLogSinkDestination.linesPerWindow

        for index in 0 ..< limit + 500 {
            send(
                .warning,
                "[truapi.product] chainHead follow p:\(index) restarted 1 ms after the stop",
                to: destination
            )
        }
        send(.info, "[truapi.chain] chain connection 2 open", to: destination)

        #expect(sink.lines.filter { $0.tag == "truapi.product" }.count == limit)
        #expect(sink.lines.last == RecordingHostLogSink.Line(
            level: .info,
            tag: "truapi.chain",
            message: "chain connection 2 open"
        ))

        clock.now += HostLogSinkDestination.window
        send(.warning, "[truapi.product] chainHead follow p:9999 restarted 1 ms after the stop", to: destination)

        #expect(Array(sink.lines.suffix(2)) == [
            RecordingHostLogSink.Line(
                level: .warning,
                tag: "truapi.product",
                message: "500 truapi.product lines dropped over the limit of \(limit) a minute",
                attributes: [HostLogSinkDestination.droppedAttribute: .count(500)]
            ),
            RecordingHostLogSink.Line(
                level: .warning,
                tag: "truapi.product",
                message: "chainHead follow p:9999 restarted 1 ms after the stop"
            )
        ])
    }

    @Test func aWindowThatDroppedNothingSendsNoCount() {
        let sink = RecordingHostLogSink()
        let clock = ManualClock()
        let destination = HostLogSinkDestination(sink: sink, now: { clock.now })

        send(.warning, "[truapi.product] first", to: destination)
        clock.now += HostLogSinkDestination.window
        send(.warning, "[truapi.product] second", to: destination)

        #expect(sink.lines.map(\.message) == ["first", "second"])
    }

    @Test func eachCoreMarkerHasItsOwnWindow() {
        let sink = RecordingHostLogSink()
        let clock = ManualClock()
        let destination = HostLogSinkDestination(sink: sink, now: { clock.now })
        let limit = HostLogSinkDestination.linesPerWindow

        for index in 0 ..< limit + 10 {
            send(.warning, "[truapi.core] truapi.tracing.warn: chain call failed \(index)", to: destination)
        }
        send(.warning, "[truapi.core] truapi.chain.follow_budget_reached: c1:product open 4s", to: destination)

        #expect(sink.lines.last?.message == "truapi.chain.follow_budget_reached: c1:product open 4s")

        clock.now += HostLogSinkDestination.window
        send(.warning, "[truapi.core] truapi.tracing.warn: chain call failed again", to: destination)

        #expect(sink.lines.suffix(2).first == RecordingHostLogSink.Line(
            level: .warning,
            tag: "truapi.core",
            message: "10 truapi.core truapi.tracing.warn lines dropped over the limit of \(limit) a minute",
            attributes: [HostLogSinkDestination.droppedAttribute: .count(10)]
        ))
    }

    @Test func coreEventsSkipTheLimit() {
        let sink = RecordingHostLogSink()
        let destination = HostLogSinkDestination(sink: sink, now: { 0 })
        let count = HostLogSinkDestination.linesPerWindow + 5

        for index in 0 ..< count {
            _ = destination.send(
                .warning,
                msg: "[truapi.core] marker \(index)",
                thread: "",
                file: "",
                function: "",
                line: 0,
                context: CoreEventContext(marker: "truapi.chain.marker", attributes: [:])
            )
        }

        #expect(sink.lines.count == count)
    }

    @Test func aTaggedLineSplitsAtTheFirstClosingBracket() {
        #expect(TaggedLogLine("[truapi.core] marker: [nested] detail") == TaggedLogLine(
            tag: "truapi.core",
            message: "marker: [nested] detail"
        ))
        #expect(TaggedLogLine("no tag here") == nil)
    }
}

private extension HostLogSinkDestinationTests {
    func send(_ level: SwiftyBeaver.Level, _ message: String, to destination: HostLogSinkDestination) {
        _ = destination.send(level, msg: message, thread: "", file: "", function: "", line: 0)
    }
}

private final class ManualClock: @unchecked Sendable {
    var now: TimeInterval = 0
}

final class RecordingHostLogSink: HostLogSink, @unchecked Sendable {
    struct Line: Equatable {
        let level: HostLogLevel
        let tag: String
        let message: String
        var attributes: [String: HostLogAttribute] = [:]
    }

    private let lock = NSLock()
    private var recorded: [Line] = []

    var lines: [Line] {
        lock.withLock { recorded }
    }

    func log(level: HostLogLevel, tag: String, message: String, attributes: [String: HostLogAttribute]) {
        lock.withLock {
            recorded.append(Line(level: level, tag: tag, message: message, attributes: attributes))
        }
    }
}
