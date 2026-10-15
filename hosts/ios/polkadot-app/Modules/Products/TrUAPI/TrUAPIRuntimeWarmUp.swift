import Foundation

/// Builds the shared core runtime away from the main thread (06-OPEN-ITEMS O24).
///
/// The core opens its database with a blocking wait when it builds the runtime
/// (`rust/crates/truapi/src/native/runtime.rs`, `Db::open`). A blocking wait on
/// the main thread inverts priorities, and the main thread stops for as long as
/// the database takes to open. The app starts this when its services start, so
/// a screen that asks for the runtime later finds it built.
///
/// The runtime needs the chains and the root entropy of the wallet, and these
/// can come after the services start. A failed build therefore tries again
/// after `interval`, at most `attempts` times.
struct TrUAPIRuntimeWarmUp: Sendable {
    var attempts = 30
    var interval: Duration = .seconds(1)
    let build: @Sendable () throws -> Void
    var logger: LoggerProtocol = Logger.shared

    /// Starts the builds on a background task. The task gives true when a build succeeds.
    @discardableResult
    func start() -> Task<Bool, Never> {
        Task.detached(priority: .userInitiated) { [self] in
            for attempt in 1 ... max(attempts, 1) {
                guard !Task.isCancelled else { return false }

                do {
                    try build()
                    return true
                } catch {
                    logger.debug("[truapi] the core runtime is not built yet (attempt \(attempt)): \(error)")
                }

                guard attempt < attempts else { break }
                try? await Task.sleep(for: interval)
            }

            logger.warning("[truapi] the core runtime was not built in advance; the first screen that asks builds it")
            return false
        }
    }
}
