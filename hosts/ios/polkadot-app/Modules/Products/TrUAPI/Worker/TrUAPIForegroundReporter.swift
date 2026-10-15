import Foundation
import TrUAPIHost

/// What receives the foreground state of the app: the core runtime.
protocol TrUAPIAppForegroundReceiving: Sendable {
    func setAppForeground(foreground: Bool)
}

extension TrUAPIHostRuntime: TrUAPIAppForegroundReceiving {}

/// Tells the core whether the app is in the foreground (VERBOSITY-WASM-PRODUCT I5).
///
/// The core decides which workers run: while the app is in the foreground it holds the worker of each product that
/// declares a `foreground` worker, and in the background it releases that hold. The host only reports the state.
/// The core starts in the background, so the reporter sends the state when the runtime is built and at each change.
///
/// Each report runs on one serial queue and reads the state at that time, so the last report always carries the
/// last state, also when a change and the build of the runtime come at the same time.
final class TrUAPIForegroundReporter: @unchecked Sendable {
    private let monitor: AppForegroundMonitor
    private let runtime: @Sendable () -> (any TrUAPIAppForegroundReceiving)?
    private let workers: @Sendable () -> (any TrUAPIWorkerManaging)?
    private let queue: DispatchQueue
    private let logger: LoggerProtocol
    /// The last state that went to the core. Read and written only on `queue`.
    private var reported: Bool?

    /// - Parameters:
    ///   - runtime: The runtime when it exists, and nil before it is built. It does not build the runtime.
    ///   - workers: The worker manager, which starts again the workers that ended on a fault.
    init(
        monitor: AppForegroundMonitor,
        runtime: @escaping @Sendable () -> (any TrUAPIAppForegroundReceiving)?,
        workers: @escaping @Sendable () -> (any TrUAPIWorkerManaging)?,
        queue: DispatchQueue = DispatchQueue(label: "io.parity.truapi.foreground"),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.monitor = monitor
        self.runtime = runtime
        self.workers = workers
        self.queue = queue
        self.logger = logger

        monitor.onChange { [weak self] _ in self?.report() }
    }

    /// Sends the state of the app to the core when it differs from the last report. The provider calls it when
    /// the runtime is built, and the monitor at each change.
    func report() {
        queue.async { [self] in
            guard let runtime = runtime() else { return }

            let foreground = monitor.isForeground
            guard foreground != reported else { return }

            reported = foreground
            runtime.setAppForeground(foreground: foreground)
            logger.debug("[truapi] the app is in the \(foreground ? "foreground" : "background")")
            workers()?.appForegroundChanged(foreground)
        }
    }

    /// Waits until each report that started before this call is done. For tests.
    func waitForReports() {
        queue.sync {}
    }
}
