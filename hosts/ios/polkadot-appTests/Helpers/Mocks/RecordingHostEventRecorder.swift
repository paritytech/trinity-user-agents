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
    /// `timeout` so an event that never comes cannot hang the test.
    func waitForEvents(count: Int, timeout: Duration = .hangGuard) async throws {
        do {
            try await withTimeout(timeout) { [self] in
                guard events.count < count else {
                    return
                }
                for await _ in eventStream where events.count >= count {
                    return
                }
            }
        } catch is TimeoutError {
            throw EventsNotRecorded(expected: count, recorded: events)
        }
    }
}
