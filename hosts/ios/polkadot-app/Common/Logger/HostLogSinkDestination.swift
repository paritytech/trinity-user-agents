import Foundation
import IssueMonitoring
import SwiftyBeaver

/// Forwards `[truapi.*]` lines at info and above to a `HostLogSink`, as the Android host does with its
/// `truapi.*` Timber tags. A core event arrives with a `CoreEventContext` and reaches the sink as its marker,
/// with its attributes as fields. Every other line stays on the device.
///
/// Each tag sends at most `linesPerWindow` host lines per `window`. A `truapi.core` line counts against its
/// marker rather than the whole tag, so one noisy marker cannot hide the others. The first line after a
/// window that dropped lines is preceded by one line that counts them, so a product or connection stuck in
/// a loop sends at most that many lines a minute plus the count. A count that no later line follows, because
/// the tag went quiet or the app stopped, is never sent. Core events are not limited here, as on Android.
final class HostLogSinkDestination: BaseDestination {
    static let linesPerWindow = 60
    static let window: TimeInterval = 60
    static let droppedAttribute = "dropped"
    static let markerAttribute = "marker"

    private struct Window {
        var start: TimeInterval
        var sent = 0
        var dropped = 0
    }

    private let sink: HostLogSink
    private let now: () -> TimeInterval
    private let lock = NSLock()
    private var windows: [String: Window] = [:]

    init(sink: HostLogSink, now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.sink = sink
        self.now = now
        super.init()
        minLevel = .info
    }

    override func send(
        _ level: SwiftyBeaver.Level,
        msg: String,
        thread _: String,
        file _: String,
        function _: String,
        line _: Int,
        context: Any? = nil
    ) -> String? {
        guard let line = TaggedLogLine(msg), line.tag.hasPrefix("truapi.") else { return nil }

        if let event = context as? CoreEventContext {
            sink.log(level: level.hostLogLevel, tag: line.tag, message: event.marker, attributes: event.attributes)
            return nil
        }
        let marker = Self.coreMarker(of: line)
        let key = marker.map { "\(line.tag) \($0)" } ?? line.tag
        guard let dropped = admit(key: key) else { return nil }
        if dropped > 0 {
            var attributes: [String: HostLogAttribute] = [Self.droppedAttribute: .count(dropped)]
            if let marker { attributes[Self.markerAttribute] = .text(marker) }
            sink.log(
                level: .warning,
                tag: line.tag,
                message: "\(dropped) \(key) lines dropped over the limit of \(Self.linesPerWindow) a minute",
                attributes: attributes
            )
        }
        sink.log(level: level.hostLogLevel, tag: line.tag, message: line.message)
        return nil
    }

    /// The marker a `truapi.core` line counts against, or nil for every other line.
    private static func coreMarker(of line: TaggedLogLine) -> String? {
        guard line.tag == "truapi.core", let markerEnd = line.message.range(of: ": "),
              markerEnd.lowerBound > line.message.startIndex
        else {
            return nil
        }
        return String(line.message[..<markerEnd.lowerBound])
    }

    /// Counts a line against its window. Returns nil when the line is over the limit, else how many lines
    /// the window that just closed dropped.
    private func admit(key: String) -> Int? {
        lock.withLock {
            let time = now()
            var window = windows[key] ?? Window(start: time)
            var closedWithDrops = 0
            if time - window.start >= Self.window {
                closedWithDrops = window.dropped
                window = Window(start: time)
            }
            defer { windows[key] = window }
            guard window.sent < Self.linesPerWindow else {
                window.dropped += 1
                return nil
            }
            window.sent += 1
            return closedWithDrops
        }
    }
}

/// A message written as `[<tag>] <message>`, the shape `TaggedLogger` and the TrUAPI code produce.
struct TaggedLogLine: Equatable {
    let tag: String
    let message: String

    init?(_ text: String) {
        guard
            text.hasPrefix("["),
            let tagEnd = text.range(of: "] ")
        else {
            return nil
        }

        tag = String(text[text.index(after: text.startIndex) ..< tagEnd.lowerBound])
        message = String(text[tagEnd.upperBound...])
    }

    init(tag: String, message: String) {
        self.tag = tag
        self.message = message
    }
}

private extension SwiftyBeaver.Level {
    var hostLogLevel: HostLogLevel {
        switch self {
        case .verbose,
             .debug,
             .info:
            .info
        case .warning:
            .warning
        case .error,
             .critical,
             .fault:
            .error
        }
    }
}
