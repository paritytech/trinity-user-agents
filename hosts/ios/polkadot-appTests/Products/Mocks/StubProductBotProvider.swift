import AsyncExtensions
import Foundation
@testable import polkadot_app

/// Serves the events a test emits, in order.
final class StubProductBotProvider: ProductBotProviding {
    private let stream: AsyncStream<ProductBotEvent>
    private let continuation: AsyncStream<ProductBotEvent>.Continuation

    init() {
        let (stream, continuation) = AsyncStream<ProductBotEvent>.makeStream()
        self.stream = stream
        self.continuation = continuation
    }

    func observeBots() -> AnyAsyncSequence<ProductBotEvent> {
        stream.eraseToAnyAsyncSequence()
    }

    func emit(_ event: ProductBotEvent) {
        continuation.yield(event)
    }
}
