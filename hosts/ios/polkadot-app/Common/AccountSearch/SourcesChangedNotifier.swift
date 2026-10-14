import Foundation
import AsyncExtensions

/// Bridges imperative source updates into the sequence returned by `sourcesChanged()`.
final class SourcesChangedNotifier {
    private let stream: AsyncStream<Void>
    private let continuation: AsyncStream<Void>.Continuation

    init() {
        (stream, continuation) = AsyncStream.makeStream()
    }

    func notify() {
        continuation.yield(())
    }

    func finish() {
        continuation.finish()
    }

    func sequence() -> AnyAsyncSequence<Void> {
        stream.eraseToAnyAsyncSequence()
    }
}
