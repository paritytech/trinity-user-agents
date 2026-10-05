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
    private var pendingTask: BGProcessingTask?

    private init() {}

    /// Registers the launch handler. Call before the app finishes launching.
    func register() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: Self.identifier, using: nil) { [weak self] task in
            guard let task = task as? BGProcessingTask else {
                task.setTaskCompleted(success: false)
                return
            }
            self?.handle(task)
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

        if let pending {
            run(pending, on: runtimeProvider)
        }
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
    func handle(_ task: BGProcessingTask) {
        lock.lock()
        let provider = runtimeProvider
        if provider == nil {
            pendingTask = task
        }
        lock.unlock()

        if let provider {
            run(task, on: provider)
        }
    }

    func run(_ task: BGProcessingTask, on runtimeProvider: TrUAPIHostRuntimeProviding) {
        let work = Task { [weak self] in
            do {
                try await runtimeProvider.sharedRuntime().runDurableRecovery()
                task.setTaskCompleted(success: true)
            } catch {
                self?.schedule()
                task.setTaskCompleted(success: false)
            }
        }
        task.expirationHandler = { work.cancel() }
    }
}
