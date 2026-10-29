import Foundation

public protocol SharedRunning<Value>: Sendable {
    associatedtype Value: Sendable

    /// Joins the run in flight or starts `operation`. A cancelled caller gets the cancelled value at once;
    /// the run itself is cancelled only when no caller is left.
    func run(_ operation: @escaping @Sendable () async -> Value) async -> Value
}

/// Unlike ``CoalescingTask``, the shared work is cancelled once every caller has been cancelled.
public actor SharedRun<Value: Sendable> {
    fileprivate typealias Waiter = CheckedContinuation<Value, Never>

    private struct InFlight {
        let id: UUID
        let task: Task<Void, Never>
        var waiters: [UUID: Waiter]
    }

    private let cancelledValue: Value
    private let onJoin: @Sendable (_ waiterCount: Int) -> Void
    private var inFlight: InFlight?

    public init(cancelledValue: Value, onJoin: @escaping @Sendable (_ waiterCount: Int) -> Void = { _ in }) {
        self.cancelledValue = cancelledValue
        self.onJoin = onJoin
    }
}

extension SharedRun: SharedRunning {
    public func run(_ operation: @escaping @Sendable () async -> Value) async -> Value {
        let waiterId = UUID()

        return await withTaskCancellationHandler {
            await withCheckedContinuation { waiter in
                join(waiterId, waiter: waiter, operation: operation)
            }
        } onCancel: {
            Task { await self.leave(waiterId) }
        }
    }
}

private extension SharedRun {
    func join(_ waiterId: UUID, waiter: Waiter, operation: @escaping @Sendable () async -> Value) {
        // The cancellation handler may have fired before this caller was registered.
        guard !Task.isCancelled else {
            waiter.resume(returning: cancelledValue)
            return
        }

        if inFlight == nil {
            let runId = UUID()
            let task = Task {
                let value = await operation()
                finish(runId, value: value)
            }
            inFlight = InFlight(id: runId, task: task, waiters: [:])
        }

        inFlight?.waiters[waiterId] = waiter
        onJoin(inFlight?.waiters.count ?? 0)
    }

    func leave(_ waiterId: UUID) {
        guard let waiter = inFlight?.waiters.removeValue(forKey: waiterId) else { return }

        waiter.resume(returning: cancelledValue)

        if inFlight?.waiters.isEmpty == true {
            inFlight?.task.cancel()
            inFlight = nil
        }
    }

    /// A run abandoned by every caller was already detached, so its late result is dropped.
    func finish(_ runId: UUID, value: Value) {
        guard let run = inFlight, run.id == runId else { return }

        inFlight = nil
        run.waiters.values.forEach { $0.resume(returning: value) }
    }
}
