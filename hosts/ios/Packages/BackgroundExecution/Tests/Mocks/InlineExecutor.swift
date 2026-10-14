import BackgroundExecution

/// Runs the operation inline so a test controls timing without an OS background assertion.
struct InlineExecutor: BackgroundExecuting {
    func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await operation()
    }
}
