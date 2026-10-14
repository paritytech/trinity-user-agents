import ChainRegistry
import Foundation

/// A ``BackgroundExecuting`` decorator that keeps every chain connection alive for the
/// duration of the operation.
public struct ConnectionRetainingExecutor {
    private let innerExecutor: BackgroundExecuting
    private let retentionProvider: ConnectionRetentionProviding

    public init(
        provider: any ConnectionRetentionProviding,
        innerExecutor: BackgroundExecuting = BackgroundExecutor()
    ) {
        self.innerExecutor = innerExecutor
        retentionProvider = provider
    }
}

// MARK: - BackgroundExecuting

extension ConnectionRetainingExecutor: @unchecked Sendable, BackgroundExecuting {
    public func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let token = retentionProvider.retainConnections(.all)
        defer { token.release() }

        return try await innerExecutor.execute(operation)
    }
}
