import Foundation
import os

/// Runs the core's calls one after another, off the caller's thread, in the
/// order they were made. Each call is queued the moment it is made, so two
/// calls from separate tasks still reach the core in the order the user made
/// them; a task-per-call alone would let the later one overtake.
final class FundingCoreCalls: Sendable {
    private let latest = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)

    /// Queues `work` after every call made before it.
    @discardableResult
    func run<Answer: Sendable>(_ work: @escaping @Sendable () -> Answer) -> Task<Answer, Never> {
        latest.withLock { latest in
            let previous = latest
            let call = Task {
                await previous?.value
                return work()
            }
            latest = Task { _ = await call.value }
            return call
        }
    }
}
