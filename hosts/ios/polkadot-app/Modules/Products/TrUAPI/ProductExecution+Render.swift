import Foundation
import TrUAPIHost

extension TrUAPIProductExecutionProtocol {
    /// Open a render stream, waiting out the window in which the product's
    /// worker is up but has not registered its renderer yet.
    ///
    /// That window is the only transient answer: a worker connects and only
    /// then subscribes, so the first renders are refused as `NotConnected` and
    /// are worth asking again for. Anything else is the product's own answer,
    /// and is raised at once because asking again cannot change it.
    func renderWhenConnected(
        _ request: ProductRendererRenderRequest,
        until deadline: ContinuousClock.Instant,
        retryEvery interval: Duration
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
        while true {
            do {
                return try render(request)
            } catch let error where (error as? ProductRuntimeError) == .NotConnected {
                guard ContinuousClock.now < deadline else { throw error }
                try await Task.sleep(for: interval)
            }
        }
    }
}
