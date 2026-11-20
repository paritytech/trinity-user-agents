import Foundation

/// Sends tagged host log lines to the reporting backend, the same lines on iOS and Android.
public protocol HostLogSink: Sendable {
    /// `attributes` arrive beside `tag` as fields the backend can filter on.
    func log(level: HostLogLevel, tag: String, message: String, attributes: [String: HostLogAttribute])
}

public extension HostLogSink {
    func log(level: HostLogLevel, tag: String, message: String) {
        log(level: level, tag: tag, message: message, attributes: [:])
    }
}

public enum HostLogLevel: Sendable {
    case info
    case warning
    case error
}

public enum HostLogAttribute: Sendable, Equatable {
    case text(String)
    case count(Int)
    case flag(Bool)
}
