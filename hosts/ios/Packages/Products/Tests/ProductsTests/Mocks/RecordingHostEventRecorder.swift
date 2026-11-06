import Foundation
import IssueMonitoring
import os

final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let recorded = OSAllocatedUnfairLock(initialState: [HostEvent]())

    var events: [HostEvent] { recorded.withLock { $0 } }

    func record(_ event: @autoclosure () -> HostEvent) {
        let event = event()
        recorded.withLock { $0.append(event) }
    }
}

extension HostLoadTelemetry {
    static func recording(
        into recorder: RecordingHostEventRecorder,
        sessions: HostSessionRegistry = HostSessionRegistry(),
        clockStep: Int = 10
    ) -> HostLoadTelemetry {
        let ticks = OSAllocatedUnfairLock(initialState: 0)

        return HostLoadTelemetry(
            recorder: recorder,
            sessions: sessions,
            monotonicMilliseconds: {
                ticks.withLock { tick in
                    tick += clockStep
                    return tick
                }
            }
        )
    }
}
