import Foundation
import IssueMonitoring

/// Records the troubleshooting trail for a product's game reminder through the shared host-event path.
struct GameReminderDiagnostics: Sendable {
    private let recorder: any HostEventRecording
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int

    init(
        recorder: any HostEventRecording = NoopHostEventRecorder(),
        origin: HostEventOrigin = .current,
        monotonicMilliseconds: @escaping @Sendable () -> Int = HostEventClock.monotonicMilliseconds
    ) {
        self.recorder = recorder
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
    }

    /// The event is built only when the recorder keeps it.
    func record(_ event: @autoclosure () -> GameReminderEvent) {
        recorder.record(event().hostEvent(origin: origin, monotonicMilliseconds: monotonicMilliseconds()))
    }
}
