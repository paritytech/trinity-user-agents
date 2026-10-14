import Foundation
import StructuredConcurrency

final class TestEvent: @unchecked Sendable {
    private struct Waiter {
        let id: UUID
        let expected: Int
        let continuation: CheckedContinuation<Void, Never>
    }

    private let lock = NSLock()
    private var storedCount = 0
    private var waiters: [Waiter] = []

    var occurrences: Int {
        lock.withLock { storedCount }
    }

    func signal() {
        let ready: [CheckedContinuation<Void, Never>] = lock.withLock {
            storedCount += 1
            let matched = waiters.filter { $0.expected <= storedCount }
            waiters.removeAll { $0.expected <= storedCount }
            return matched.map(\.continuation)
        }

        ready.forEach { $0.resume() }
    }

    func wait(occurrences expected: Int = 1, timeout: Duration = .seconds(100)) async throws {
        guard occurrences < expected else { return }

        try await withTimeout(timeout) { [self] in
            let id = UUID()

            await withTaskCancellationHandler {
                await withCheckedContinuation { continuation in
                    let resumeNow: Bool = lock.withLock {
                        guard storedCount < expected else { return true }
                        waiters.append(Waiter(id: id, expected: expected, continuation: continuation))
                        return false
                    }

                    if resumeNow { continuation.resume() }
                }
            } onCancel: {
                // withTimeout cancels the loser; an abandoned waiter must be resumed or the group hangs.
                let abandoned: CheckedContinuation<Void, Never>? = lock.withLock {
                    guard let index = waiters.firstIndex(where: { $0.id == id }) else { return nil }
                    return waiters.remove(at: index).continuation
                }

                abandoned?.resume()
            }
        }
    }
}

func settle(for duration: Duration = .milliseconds(300)) async {
    try? await Task.sleep(for: duration)
}
