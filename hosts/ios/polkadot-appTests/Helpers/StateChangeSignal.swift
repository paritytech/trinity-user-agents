import Foundation

/// Buffered and single-waiter: a change signalled before `wait(until:)` starts is not missed.
final class StateChangeSignal: @unchecked Sendable {
    private let changes: AsyncStream<Void>
    private let continuation: AsyncStream<Void>.Continuation

    init() {
        (changes, continuation) = AsyncStream.makeStream()
    }

    func signal() {
        continuation.yield()
    }

    @MainActor
    func wait(until condition: () -> Bool) async {
        guard !condition() else { return }

        for await _ in changes where condition() {
            return
        }
    }
}
