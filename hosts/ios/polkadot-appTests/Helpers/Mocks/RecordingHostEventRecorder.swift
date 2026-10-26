import Foundation
import IssueMonitoring

final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedEvents: [HostEvent] = []

    var events: [HostEvent] {
        lock.withLock { recordedEvents }
    }

    func record(_ event: @autoclosure () -> HostEvent) {
        let event = event()
        lock.withLock { recordedEvents.append(event) }
    }
}
