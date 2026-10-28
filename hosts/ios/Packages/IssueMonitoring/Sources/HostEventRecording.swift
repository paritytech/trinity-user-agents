import Foundation

/// Records native host lifecycle events.
///
/// The event is an autoclosure so a recorder that drops events, as every build
/// without reporting support uses, never builds one.
public protocol HostEventRecording: Sendable {
    func record(_ event: @autoclosure () -> HostEvent)
}

/// Drops every event without building it.
public struct NoopHostEventRecorder: HostEventRecording {
    public init() {}

    public func record(_: @autoclosure () -> HostEvent) {}
}
