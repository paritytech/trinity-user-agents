import UIKit

/// Bridges a host screen's decision to the rust-core future, resolving exactly
/// once. Rust-side cancellation answers `cancelled` without waiting for the
/// screen and closes the pending `screen`; a decision arriving afterwards is
/// discarded.
func awaitDecision<Decision: Sendable>(
    cancelled: Decision,
    present: @escaping @MainActor (PendingDecision<Decision>) async -> Decision
) async -> Decision {
    let pending = PendingDecision(cancelled: cancelled)
    return await withTaskCancellationHandler {
        await withCheckedContinuation { continuation in
            Task { @MainActor in
                guard pending.begin(continuation) else {
                    return
                }
                let verdict = await present(pending)
                pending.finish(verdict)
            }
        }
    } onCancel: {
        Task { @MainActor in pending.cancel() }
    }
}

/// Resume-once state for one host screen. All mutable state is
/// MainActor-confined; a cancellation racing ahead of `begin` resolves the
/// incoming continuation with `cancelled` instead of leaking it.
@MainActor
final class PendingDecision<Decision: Sendable> {
    /// Closed when the core stops waiting before it answers. A decision with
    /// no screen leaves its prompt up and discards the late answer.
    weak var screen: UIViewController?
    private(set) var isFinished = false
    private var continuation: CheckedContinuation<Decision, Never>?
    private let cancelled: Decision

    nonisolated init(cancelled: Decision) {
        self.cancelled = cancelled
    }

    /// Returns false when the decision already finished (e.g. cancelled
    /// before the screen task ran); the continuation is resolved either way.
    func begin(_ continuation: CheckedContinuation<Decision, Never>) -> Bool {
        guard !isFinished else {
            continuation.resume(returning: cancelled)
            return false
        }
        self.continuation = continuation
        return true
    }

    func finish(_ verdict: Decision) {
        guard !isFinished else {
            return
        }
        isFinished = true
        continuation?.resume(returning: verdict)
        continuation = nil
    }

    func cancel() {
        guard !isFinished else {
            return
        }
        finish(cancelled)
        screen?.dismiss(animated: true)
    }
}
