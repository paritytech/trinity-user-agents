import BackgroundExecution
import BackgroundTasks
import Foundation
import os
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

    private struct State {
        var runtimeProvider: TrUAPIHostRuntimeProviding?
        var pendingTask: CoreDurableRecoveryRun?
        var foreground: CoreDurableForegroundRecovery?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

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
        let foreground = CoreDurableForegroundRecovery(runtimeProvider: runtimeProvider, executor: executor)
        let pending = state.withLock { state in
            state.runtimeProvider = runtimeProvider
            state.foreground = foreground
            let pending = state.pendingTask
            state.pendingTask = nil
            return pending
        }

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
        let foreground = state.withLock { $0.foreground }
        foreground?.request()
    }
}

private extension CoreDurableRecoveryTask {
    func handle(_ run: CoreDurableRecoveryRun) {
        let (provider, replaced): (TrUAPIHostRuntimeProviding?, CoreDurableRecoveryRun?) = state.withLock { state in
            guard let provider = state.runtimeProvider else {
                let replaced = state.pendingTask
                state.pendingTask = run
                return (nil, replaced)
            }
            return (provider, nil)
        }

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
    private struct State {
        var finished = false
        var work: Task<Void, Never>?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

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
        state.withLock { $0.work = run }
    }

    /// Completes the system task once; a failed run asks for the next one.
    /// Drops the work it holds, which holds this run in turn.
    func finish(success: Bool) {
        let first = state.withLock { state in
            let first = !state.finished
            state.finished = true
            state.work = nil
            return first
        }

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
        let run = state.withLock { $0.work }
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
    private struct State {
        var running = false
        var requested = false

        /// Whether the caller starts the requested run.
        mutating func claimRun() -> Bool {
            guard requested, !running else {
                return false
            }
            requested = false
            running = true
            return true
        }
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    init(runtimeProvider: TrUAPIHostRuntimeProviding, executor: BackgroundExecuting) {
        self.runtimeProvider = runtimeProvider
        self.executor = executor
    }

    func request() {
        let starts = state.withLock { state in
            state.requested = true
            return state.claimRun()
        }

        if starts {
            start()
        }
    }
}

private extension CoreDurableForegroundRecovery {
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
        let starts = state.withLock { state in
            state.running = false
            return state.claimRun()
        }

        if starts {
            start()
        }
    }
}
