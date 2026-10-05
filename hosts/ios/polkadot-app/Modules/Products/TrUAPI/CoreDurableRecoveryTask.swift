import BackgroundTasks
import Foundation
import TrUAPIHost

/// Keeps the core's durable recovery running while it has transactions
/// awaiting a verdict. The core asks for it through
/// ``HostBridge/durableWorkChanged(pending:)``; a run awaits
/// ``TrUAPIHostRuntime/runDurableRecovery()``, which returns once nothing is
/// live. A run that stops early or expires schedules the next one.
///
/// The handler is registered at launch, before the runtime provider exists,
/// so a task delivered early waits for ``attach(_:)``.
final class CoreDurableRecoveryTask: @unchecked Sendable {
    static let identifier = "io.novatech.truapi.durable.recovery"

    static let shared = CoreDurableRecoveryTask()

    private let lock = NSLock()
    private var runtimeProvider: TrUAPIHostRuntimeProviding?
    private var pendingTask: CoreDurableRecoveryRun?

    private init() {}

    /// Registers the launch handler. Call before the app finishes launching.
    func register() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: Self.identifier, using: nil) { [weak self] task in
            self?.handle(CoreDurableRecoveryRun(task: task, reschedule: { self?.schedule() }))
        }
    }

    /// Supplies the runtime a run recovers on, and starts a task that arrived
    /// before it.
    func attach(_ runtimeProvider: TrUAPIHostRuntimeProviding) {
        lock.lock()
        self.runtimeProvider = runtimeProvider
        let pending = pendingTask
        pendingTask = nil
        lock.unlock()

        pending?.start(on: runtimeProvider)
    }

    /// Asks the system for a run with network access.
    func schedule() {
        let request = BGProcessingTaskRequest(identifier: Self.identifier)
        request.requiresNetworkConnectivity = true
        request.requiresExternalPower = false
        try? BGTaskScheduler.shared.submit(request)
    }
}

private extension CoreDurableRecoveryTask {
    func handle(_ run: CoreDurableRecoveryRun) {
        lock.lock()
        let provider = runtimeProvider
        let replaced = provider == nil ? pendingTask : nil
        if provider == nil {
            pendingTask = run
        }
        lock.unlock()

        replaced?.finish(success: false)
        if let provider {
            run.start(on: provider)
        }
    }
}

/// One delivered background task, completed exactly once: by the recovery
/// run, or by expiry. Cancelling the Swift task does not stop the run in the
/// core, so expiry completes the system task itself instead of waiting for
/// the run to notice.
private final class CoreDurableRecoveryRun: @unchecked Sendable {
    private let task: BGTask
    private let reschedule: () -> Void
    private let lock = NSLock()
    private var finished = false
    private var work: Task<Void, Never>?

    init(task: BGTask, reschedule: @escaping () -> Void) {
        self.task = task
        self.reschedule = reschedule
        task.expirationHandler = { [weak self] in
            self?.expire()
        }
    }

    func start(on runtimeProvider: TrUAPIHostRuntimeProviding) {
        // Holds the run strongly: nothing else does once the task has started.
        let run = Task { [self] in
            do {
                try await runtimeProvider.sharedRuntime().runDurableRecovery()
                finish(success: true)
            } catch {
                finish(success: false)
            }
        }
        lock.lock()
        work = run
        lock.unlock()
    }

    /// Completes the system task once; a failed run asks for the next one.
    func finish(success: Bool) {
        lock.lock()
        let first = !finished
        finished = true
        lock.unlock()

        guard first else {
            return
        }
        if !success {
            reschedule()
        }
        task.setTaskCompleted(success: success)
    }
}

private extension CoreDurableRecoveryRun {
    func expire() {
        lock.lock()
        let run = work
        lock.unlock()

        run?.cancel()
        finish(success: false)
    }
}
