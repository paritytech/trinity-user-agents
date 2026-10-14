import Foundation
import SDKLogger

/// Actor managing sequential payment execution with a pending queue.
///
/// Stores pending tasks with their execution closures. Only one payment
/// runs at a time; when it completes the next pending is started automatically.
actor ExternalPaymentContext {
    struct Pending {
        let paymentId: String
        let onExecute: @Sendable () -> Task<Void, Never>
    }

    private(set) var currentPaymentId: String?
    private var currentTask: Task<Void, Never>?
    private var pendingTasks: [Pending] = []
    private let logger: SDKLoggerProtocol?

    init(logger: SDKLoggerProtocol? = nil) {
        self.logger = logger
    }

    /// Enqueues a payment for processing. Starts immediately if idle.
    /// Duplicate ids (already processing or already pending) are ignored.
    func scheduleIfNeeded(
        paymentId: String,
        onExecute: @escaping @Sendable () -> Task<Void, Never>
    ) {
        guard paymentId != currentPaymentId,
              !pendingTasks.contains(where: { $0.paymentId == paymentId })
        else {
            return
        }

        if currentPaymentId == nil {
            startProcessing(Pending(paymentId: paymentId, onExecute: onExecute))
        } else {
            pendingTasks.append(Pending(paymentId: paymentId, onExecute: onExecute))
            logger?.debug("Queued payment \(paymentId), pending: \(pendingTasks.count)")
        }
    }

    /// Called when payment execution finishes. Starts the next pending payment.
    func onComplete(paymentId: String) {
        guard currentPaymentId == paymentId else { return }

        currentTask = nil
        currentPaymentId = nil

        startNextPendingIfNeeded()
    }
}

// MARK: - Private

private extension ExternalPaymentContext {
    func startProcessing(_ pending: Pending) {
        currentPaymentId = pending.paymentId
        currentTask = pending.onExecute()
        logger?.debug("Started processing payment \(pending.paymentId)")
    }

    func startNextPendingIfNeeded() {
        guard !pendingTasks.isEmpty else { return }

        let next = pendingTasks.removeFirst()
        startProcessing(next)
    }
}
