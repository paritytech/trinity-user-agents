import Foundation
import Products
import UIKit

/// Keeps the app active in the background while the core holds a worker for a wake (07-LATER §10.2, O30).
protocol ProductWakeBackgroundTasking: Sendable {
    /// Runs a background task for the product until `deadline`. A later deadline moves the end of the task.
    @MainActor
    func hold(productId: ProductId, until deadline: Date)
}

/// Starts and ends the UIKit background tasks of ``ProductWakeBackgroundTasks``.
@MainActor
protocol BackgroundTaskRunning {
    /// Starts a background task. The system calls `expiration` when the background time of the app ends.
    func begin(name: String, expiration: @escaping @MainActor () -> Void) -> UIBackgroundTaskIdentifier

    func end(_ identifier: UIBackgroundTaskIdentifier)
}

/// The background tasks of `UIApplication`.
struct ApplicationBackgroundTasks: BackgroundTaskRunning {
    // The protocol isolates the type to the main actor, and the default argument of a nonisolated init needs it.
    // swiftlint:disable:next unneeded_synthesized_initializer
    nonisolated init() {}

    func begin(name: String, expiration: @escaping @MainActor () -> Void) -> UIBackgroundTaskIdentifier {
        UIApplication.shared.beginBackgroundTask(withName: name) {
            MainActor.assumeIsolated { expiration() }
        }
    }

    func end(_ identifier: UIBackgroundTaskIdentifier) {
        UIApplication.shared.endBackgroundTask(identifier)
    }
}

/// One UIKit background task for each product that the core holds for a wake. The task ends at the end of the hold,
/// or when the system ends the background time of the app. iOS can suspend the app before the end of the hold, so a
/// wake in the background is best effort (O30).
@MainActor
final class ProductWakeBackgroundTasks: ProductWakeBackgroundTasking {
    private struct Running {
        let identifier: UIBackgroundTaskIdentifier
        let deadline: Date
        let timer: Task<Void, Never>
    }

    private let runner: BackgroundTaskRunning
    private let now: () -> Date
    private var running: [ProductId: Running] = [:]

    nonisolated init(
        runner: BackgroundTaskRunning = ApplicationBackgroundTasks(),
        now: @escaping () -> Date = Date.init
    ) {
        self.runner = runner
        self.now = now
    }

    /// The products that have a background task now.
    var heldProducts: [ProductId] {
        running.keys.sorted()
    }

    func hold(productId: ProductId, until deadline: Date) {
        let previous = running[productId]
        if let previous, previous.deadline >= deadline {
            return
        }
        previous?.timer.cancel()

        let identifier = previous?.identifier ?? runner.begin(name: "truapi.wake.\(productId)") { [weak self] in
            self?.end(productId)
        }
        guard identifier != .invalid else { return }

        let wait = max(deadline.timeIntervalSince(now()), 0)
        let timer = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(wait * 1_000)))
            guard !Task.isCancelled else { return }
            self?.end(productId)
        }
        running[productId] = Running(identifier: identifier, deadline: deadline, timer: timer)
    }

    private func end(_ productId: ProductId) {
        guard let ended = running.removeValue(forKey: productId) else { return }
        ended.timer.cancel()
        runner.end(ended.identifier)
    }
}
