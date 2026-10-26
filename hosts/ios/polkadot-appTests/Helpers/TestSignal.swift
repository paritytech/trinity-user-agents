import StructuredConcurrency
import StructuredConcurrencyTestSupport

/// An event a test waits on. A test double fires it at the call under test, and `next()` returns once a fire
/// is pending, so the test resumes on the event itself rather than polling for state.
struct TestSignal: Sendable {
    private let stream: AsyncStream<Void>
    private let continuation: AsyncStream<Void>.Continuation

    init() {
        (stream, continuation) = AsyncStream.makeStream()
    }

    func fire() {
        continuation.yield()
    }

    /// Returns once `fire()` has been called and not yet consumed, failing rather than hanging if it never is.
    func next() async throws {
        try await withTimeout(.hangGuard) { [stream] in
            for await _ in stream {
                return
            }
            throw CancellationError()
        }
    }
}
