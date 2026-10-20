import Foundation
import os
import ChainRegistry

/// Runs queue hops inline and holds delayed work until a test fires it by delay.
final class ManualChainSocketScheduler: ChainSocketScheduling, @unchecked Sendable {
    private let scheduled = OSAllocatedUnfairLock<[(delay: TimeInterval, work: DispatchWorkItem)]>(
        uncheckedState: []
    )

    /// Delays of scheduled work that is still pending.
    var pendingDelays: [TimeInterval] {
        scheduled.withLockUnchecked { $0.filter { !$0.work.isCancelled }.map(\.delay) }
    }

    func async(on _: DispatchQueue, execute work: DispatchWorkItem) {
        work.perform()
    }

    func schedule(after delay: TimeInterval, on _: DispatchQueue, execute work: DispatchWorkItem) {
        scheduled.withLockUnchecked { $0.append((delay, work)) }
    }

    /// Runs pending work scheduled with `delay`; work it schedules in turn waits for a later call.
    func fire(after delay: TimeInterval) {
        let due = scheduled.withLockUnchecked { scheduled -> [DispatchWorkItem] in
            let due = scheduled.filter { $0.delay == delay }.map(\.work)
            scheduled.removeAll { $0.delay == delay }
            return due
        }

        due.filter { !$0.isCancelled }.forEach { $0.perform() }
    }
}
