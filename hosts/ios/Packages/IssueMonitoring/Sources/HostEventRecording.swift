import Foundation

public protocol HostEventRecording: Sendable {
    func record(_ event: HostEvent)
}

public struct NoopHostEventRecorder: HostEventRecording {
    public init() {}

    public func record(_: HostEvent) {}
}
