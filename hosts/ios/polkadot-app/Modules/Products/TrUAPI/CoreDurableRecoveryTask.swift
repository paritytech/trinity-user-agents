import BackgroundExecution
import BackgroundTasks
import Foundation
import TrUAPIHost

/// Keeps the core's durable recovery running while it has transactions
/// awaiting a verdict. The core asks for it through
/// ``HostBridge/durableWorkChanged(pending:)``; a run awaits
/// ``TrUAPIHostRuntime/runDurableRecovery()``, which returns once nothing is
/// live. A run that stops early or expires schedules the next one.
///
/// The system delivers no background task while the app is in the
/// foreground, so a request also starts a run in-process right away.
///
/// The handler is registered at launch, before the runtime provider exists,
/// so a task delivered early waits for ``attach(_:executor:)``.
final class CoreDurableRecoveryTask: @unchecked Sendable {
    static let identifier = "io.novatech.truapi.durable.recovery"

    static let shared = CoreDurableRecoveryTask()

    private let lock = NSLock()
    private var runtimeProvider: TrUAPIHostRuntimeProviding?
    private var pendingTask: CoreDurableRecoveryRun?
    private var foreground: CoreDurableForegroundRecovery?

    private init() {}

    /// Registers the launch handler. Call before the app finishes launching.
    func register() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: Self.identifier, using: nil) { [weak self] task in
            self?.handle(CoreDurableRecoveryRun(task: task, reschedule: { self?.schedule() }))
        }
    }

    /// Supplies the runtime a run recovers on and the executor an in-process
    /// run holds its background time through, and starts a task that arrived
    /// before them.
    func attach(_ runtimeProvider: TrUAPIHostRuntimeProviding, executor: BackgroundExecuting) {
        lock.lock()
        self.runtimeProvider = runtimeProvider
        foreground = CoreDurableForegroundRecovery(runtimeProvider: runtimeProvider, executor: executor)
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

    /// Starts a run in-process. Ignored before ``attach(_:executor:)``, when
    /// no runtime exists to have work.
    func runInForeground() {
        lock.lock()
        let foreground = self.foreground
        lock.unlock()

        foreground?.request()
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

/// In-process recovery runs, one at a time, each holding background time so
/// that leaving the app does not cut it short. A request during a run starts
/// one more after it, because the run may have read the ledger before the
/// work behind the request was written.
private final class CoreDurableForegroundRecovery: @unchecked Sendable {
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let executor: BackgroundExecuting
    private let lock = NSLock()
    private var running = false
    private var requested = false

    init(runtimeProvider: TrUAPIHostRuntimeProviding, executor: BackgroundExecuting) {
        self.runtimeProvider = runtimeProvider
        self.executor = executor
    }

    func request() {
        lock.lock()
        requested = true
        let starts = claimRun()
        lock.unlock()

        if starts {
            start()
        }
    }
}

private extension CoreDurableForegroundRecovery {
    /// Called with the lock held: whether the caller starts the requested run.
    func claimRun() -> Bool {
        guard requested, !running else {
            return false
        }
        requested = false
        running = true
        return true
    }

    /// A failed run needs no retry here: the scheduled background task
    /// covers it.
    func start() {
        Task { [self] in
            try? await executor.execute { [runtimeProvider] in
                try await runtimeProvider.sharedRuntime().runDurableRecovery()
            }
            finish()
        }
    }

    func finish() {
        lock.lock()
        running = false
        let starts = claimRun()
        lock.unlock()

        if starts {
            start()
        }
    }
}
