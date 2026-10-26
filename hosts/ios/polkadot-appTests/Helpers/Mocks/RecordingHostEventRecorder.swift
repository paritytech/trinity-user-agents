import Foundation
import IssueMonitoring

final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedEvents: [HostEvent] = []

    var events: [HostEvent] {
        lock.withLock { recordedEvents }
    }

    func record(_ event: HostEvent) {
        lock.withLock { recordedEvents.append(event) }
    }
}
