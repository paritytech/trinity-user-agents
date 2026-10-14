import AsyncExtensions
import Foundation
@testable import polkadot_app

/// Serves the product snapshots a test emits, in order.
final class StubProductBotProvider: ProductBotProviding {
    private let stream: AsyncStream<ProductBots>
    private let continuation: AsyncStream<ProductBots>.Continuation

    init() {
        let (stream, continuation) = AsyncStream<ProductBots>.makeStream()
        self.stream = stream
        self.continuation = continuation
    }

    func observeBots() -> AnyAsyncSequence<ProductBots> {
        stream.eraseToAnyAsyncSequence()
    }

    func emit(_ bots: ProductBots) {
        continuation.yield(bots)
    }
}
