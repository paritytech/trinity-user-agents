import Products
import StructuredConcurrency
import TrUAPIHost

typealias ProductWorkerRuntimeProvider = @Sendable () throws -> any ProductWorkerRuntime

protocol ProductWorkerRuntime: Sendable {
    func claimWorker(productId: ProductId) -> NativeWorkerClaim
    func currentWorkerExecution(productId: ProductId) -> NativeWorkerExecution?
    func workerExecutionState(productId: ProductId) -> AsyncStream<NativeWorkerExecutionState>
}

extension TrUAPIHostRuntime: ProductWorkerRuntime {}

extension Duration {
    static let workerStartupTimeout = Duration.seconds(30)
}

extension ProductWorkerRuntime {
    func waitForWorker(_ productId: ProductId, timeout: Duration = .workerStartupTimeout) async throws {
        let states = workerExecutionState(productId: productId)
        try await withTimeout(timeout) {
            for await state in states {
                switch state {
                case .running: return
                case let .failed(reason): throw HostRejection.Rejected(reason: reason)
                case .idle,
                     .pending: continue
                }
            }
            throw CancellationError()
        }
    }
}
