import Foundation
import TrUAPIHost

extension TrUAPIProductExecutionProtocol {
    /// Open a render stream once the product is attached, giving up at
    /// `deadline`.
    ///
    /// A worker connects and only then answers renders, so the execution is
    /// asked to say when its product is attached, and the product is asked
    /// once after that. Whatever that render answers is the product's own
    /// answer, `NotConnected` included once the deadline has passed.
    func renderWhenConnected(
        _ request: ProductRendererRenderRequest,
        until deadline: ContinuousClock.Instant
    ) async throws -> AsyncThrowingStream<RendererNode, Error> {
        let timeLeft = max(deadline - .now, .zero)
        await waitUntilConnected(timeoutMs: UInt32(clamping: timeLeft.wholeMilliseconds))
        return try render(request)
    }
}

private extension Duration {
    var wholeMilliseconds: Int64 {
        components.seconds * 1_000 + components.attoseconds / 1_000_000_000_000_000
    }
}
