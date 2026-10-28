import Foundation
import IssueMonitoring
import StructuredConcurrency
import StructuredConcurrencyTestSupport

final class RecordingHostEventRecorder: HostEventRecording, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedEvents: [HostEvent] = []
    private let eventStream: AsyncStream<HostEvent>
    private let eventContinuation: AsyncStream<HostEvent>.Continuation

    init() {
        (eventStream, eventContinuation) = AsyncStream.makeStream(bufferingPolicy: .unbounded)
    }

    var events: [HostEvent] {
        lock.withLock { recordedEvents }
    }

    struct EventsNotRecorded: Error {
        let expected: Int
        let recorded: [HostEvent]
    }

    func record(_ event: @autoclosure () -> HostEvent) {
        let event = event()
        lock.withLock { recordedEvents.append(event) }
        eventContinuation.yield(event)
    }

    /// Returns once `count` events are recorded, including ones recorded before the call; fails after
    /// `timeout` so an event that never comes cannot hang the test. One waiter per recorder.
    func waitForEvents(count: Int, timeout: Duration = .hangGuard) async throws -> [HostEvent] {
        do {
            try await withTimeout(timeout) { [eventStream] in
                var seen = 0
                for await _ in eventStream {
                    seen += 1
                    if seen == count {
                        return
                    }
                }
            }
        } catch is TimeoutError {
            throw EventsNotRecorded(expected: count, recorded: events)
        }
        return events
    }
}
