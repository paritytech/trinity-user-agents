import Foundation
import AsyncExtensions

actor AsyncBroadcast<Event: Sendable> {
    private var continuations = [UUID: AsyncStream<Event>.Continuation]()
    private let policy: AsyncStream<Event>.Continuation.BufferingPolicy

    init(policy: AsyncStream<Event>.Continuation.BufferingPolicy = .unbounded) {
        self.policy = policy
    }

    func newSequence() -> AnyAsyncSequence<Event> {
        let id = UUID()

        return AsyncStream(bufferingPolicy: policy) { continuation in
            continuations[id] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.remove(id) }
            }
        }
        .eraseToAnyAsyncSequence()
    }

    func yield(_ event: Event) {
        for value in continuations.values {
            value.yield(event)
        }
    }

    private func remove(_ id: UUID) {
        continuations[id] = nil
    }
}
