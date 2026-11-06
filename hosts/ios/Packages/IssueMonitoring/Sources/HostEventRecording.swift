import Foundation

/// Records native host lifecycle events.
///
/// The event is an autoclosure so a recorder that drops events, as every build
/// without reporting support uses, never builds one.
public protocol HostEventRecording: Sendable {
    /// False for a recorder that drops everything, so nothing is timed for it.
    var isRecording: Bool { get }

    func record(_ event: @autoclosure () -> HostEvent)
}

public extension HostEventRecording {
    var isRecording: Bool { true }
}

/// Drops every event without building it.
public struct NoopHostEventRecorder: HostEventRecording {
    public var isRecording: Bool { false }

    public init() {}

    public func record(_: @autoclosure () -> HostEvent) {}
}
