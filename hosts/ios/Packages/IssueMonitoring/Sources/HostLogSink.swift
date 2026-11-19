import Foundation

/// Sends tagged host log lines to the reporting backend, the same lines on iOS and Android.
public protocol HostLogSink: Sendable {
    func log(level: HostLogLevel, tag: String, message: String)
}

public enum HostLogLevel: Sendable {
    case info
    case warning
    case error
}
