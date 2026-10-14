final class DeviceSyncCancellationGate: @unchecked Sendable {
    private let stream: AsyncStream<Void>
    private let lifetime: AsyncStream<Void>.Continuation

    init() {
        (stream, lifetime) = AsyncStream.makeStream()
    }

    /// Suspends until cancellation of the awaiting task ends the stream iteration.
    func wait() async {
        for await _ in stream {
            return
        }
    }
}
