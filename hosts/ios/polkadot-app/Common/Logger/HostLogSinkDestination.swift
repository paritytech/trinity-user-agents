import Foundation
import IssueMonitoring
import SwiftyBeaver

/// Forwards `[truapi.*]` lines at info and above to a `HostLogSink`, as the Android host does with its
/// `truapi.*` Timber tags. Every other line stays on the device.
final class HostLogSinkDestination: BaseDestination {
    private let sink: HostLogSink

    init(sink: HostLogSink) {
        self.sink = sink
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
        context _: Any? = nil
    ) -> String? {
        guard let line = TaggedLogLine(msg), line.tag.hasPrefix("truapi.") else { return nil }

        sink.log(level: level.hostLogLevel, tag: line.tag, message: line.message)
        return nil
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
