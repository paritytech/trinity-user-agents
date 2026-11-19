import IssueMonitoring
import SDKLogger
import TrUAPIHost

/// Logs the core events both host bridges receive. Each event stays on the device as a debug line,
/// and a marker the core's table exports also goes to `sink` at the severity the table carries, with
/// each attribute as a field the backend filters on. The debug line stays below what
/// `HostLogSinkDestination` forwards, so an exported event reaches Sentry once, with its attributes.
struct CoreEventLogger {
    private let logger: LoggerProtocol
    private let sink: HostLogSink?

    /// `sink` is nil in builds that send nothing off the device.
    init(logger: LoggerProtocol, sink: HostLogSink?) {
        self.logger = logger
        self.sink = sink
    }

    func log(marker: String, attributes: [CoreAttribute]) {
        let fields = attributes.map { "\($0.key)=\($0.value.logText)" }.joined(separator: " ")
        logger.debug("[truapi.core] \(marker): \(fields)")

        guard let level = exportedCoreMarkerLevels[marker] else { return }

        sink?.log(
            level: level,
            tag: "truapi.core",
            message: marker,
            attributes: Dictionary(attributes.map { ($0.key, $0.value.hostLogAttribute) }) { _, last in last }
        )
    }
}

private extension CoreAttributeValue {
    var logText: String {
        switch self {
        case let .text(value):
            value
        case let .count(value):
            String(value)
        case let .flag(value):
            String(value)
        }
    }

    var hostLogAttribute: HostLogAttribute {
        switch self {
        case let .text(value):
            .text(value)
        case let .count(value):
            .count(Int(clamping: value))
        case let .flag(value):
            .flag(value)
        }
    }
}
