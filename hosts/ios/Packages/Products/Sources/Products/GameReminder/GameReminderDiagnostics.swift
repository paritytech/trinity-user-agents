import Foundation
import IssueMonitoring

/// Records the troubleshooting trail for a product's game reminder through the shared host-event path.
public struct GameReminderDiagnostics: Sendable {
    private let recorder: any HostEventRecording
    private let origin: HostEventOrigin

    public init(
        recorder: any HostEventRecording = NoopHostEventRecorder(),
        origin: HostEventOrigin = .current
    ) {
        self.recorder = recorder
        self.origin = origin
    }

    public func record(_ event: GameReminderEvent) {
        recorder.record(event.hostEvent(origin: origin))
    }
}
