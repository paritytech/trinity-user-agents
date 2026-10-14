import Foundation

public extension AsyncSequence {
    /// Debounces elements by `duration`, emitting immediately those matching `fastPath`.
    func withDebounce(
        for duration: Duration,
        fastPath: @escaping (Element) -> Bool = { _ in false }
    ) -> AsyncThrowingStream<Element, Error> {
        AsyncThrowingStream { continuation in
            let emitter = DebounceEmitter(continuation: continuation)

            let task = Task {
                var pendingTask: Task<Void, Never>?

                do {
                    for try await element in self {
                        pendingTask?.cancel()

                        if fastPath(element) {
                            pendingTask = nil
                            await emitter.emitNow(element)
                        } else {
                            let generation = await emitter.beginPending()

                            pendingTask = Task {
                                try? await Task.sleep(for: duration)
                                await emitter.emit(element, ifCurrent: generation)
                            }
                        }
                    }

                    await pendingTask?.value
                    continuation.finish()
                } catch {
                    pendingTask?.cancel()
                    continuation.finish(throwing: error)
                }
            }

            continuation.onTermination = { _ in
                task.cancel()
            }
        }
    }
}

private actor DebounceEmitter<Element> {
    private let continuation: AsyncThrowingStream<Element, Error>.Continuation

    /// Guards against stale emissions: cancel() is cooperative, so a pending task that already
    /// passed its sleep may still attempt to yield a superseded element. Every new element bumps
    /// the generation, and a pending yield goes through only if its generation is still current.
    private var generation = 0

    init(continuation: AsyncThrowingStream<Element, Error>.Continuation) {
        self.continuation = continuation
    }

    func emitNow(_ element: Element) {
        generation += 1
        continuation.yield(element)
    }

    func beginPending() -> Int {
        generation += 1
        return generation
    }

    func emit(_ element: Element, ifCurrent expectedGeneration: Int) {
        guard generation == expectedGeneration else {
            return
        }

        continuation.yield(element)
    }
}
