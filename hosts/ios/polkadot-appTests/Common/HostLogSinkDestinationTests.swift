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

final class RecordingHostLogSink: HostLogSink, @unchecked Sendable {
    struct Line: Equatable {
        let level: HostLogLevel
        let tag: String
        let message: String
    }

    private let lock = NSLock()
    private var recorded: [Line] = []

    var lines: [Line] {
        lock.withLock { recorded }
    }

    func log(level: HostLogLevel, tag: String, message: String) {
        lock.withLock { recorded.append(Line(level: level, tag: tag, message: message)) }
    }
}
