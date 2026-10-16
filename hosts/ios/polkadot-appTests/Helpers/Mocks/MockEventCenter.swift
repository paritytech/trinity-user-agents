import Foundation
import os
import EventCenter

@testable import polkadot_app

final class MockEventCenter: EventCenterProtocol, @unchecked Sendable {
    private struct EventWaiter {
        let matches: (EventProtocol) -> Bool
        let continuation: CheckedContinuation<EventProtocol, Never>
    }

    private struct State {
        var events: [EventProtocol] = []
        var waiters: [EventWaiter] = []
        var observers: [WeakEventVisitor] = []
    }

    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    func notify(with event: EventProtocol) {
        let (readyWaiters, observers) = state.withLock { state in
            state.events.append(event)

            let ready = state.waiters.filter { $0.matches(event) }
            state.waiters.removeAll { $0.matches(event) }

            state.observers.removeAll { $0.observer == nil }

            return (ready, state.observers.compactMap(\.observer))
        }

        readyWaiters.forEach { $0.continuation.resume(returning: event) }

        for observer in observers {
            event.accept(visitor: observer)
        }
    }

    /// Returns the first event matching `predicate`, including one notified before the call.
    func waitForEvent(where predicate: @escaping (EventProtocol) -> Bool) async -> EventProtocol {
        await withCheckedContinuation { continuation in
            let notified: EventProtocol? = state.withLock { state in
                if let event = state.events.first(where: predicate) {
                    return event
                }

                state.waiters.append(EventWaiter(matches: predicate, continuation: continuation))

                return nil
            }

            if let notified {
                continuation.resume(returning: notified)
            }
        }
    }

    func add(observer: EventVisitorProtocol, dispatchIn _: DispatchQueue?) {
        state.withLock { $0.observers.append(WeakEventVisitor(observer: observer)) }
    }

    func remove(observer: EventVisitorProtocol) {
        state.withLock { $0.observers.removeAll { $0.observer === observer } }
    }
}

private final class WeakEventVisitor {
    weak var observer: EventVisitorProtocol?

    init(observer: EventVisitorProtocol) {
        self.observer = observer
    }
}
