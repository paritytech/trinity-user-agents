import BackgroundExecution

/// Runs the operation inline, no OS background assertion — deterministic for tests.
struct InlineBackgroundExecutor: BackgroundExecuting {
    func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await operation()
    }
}
