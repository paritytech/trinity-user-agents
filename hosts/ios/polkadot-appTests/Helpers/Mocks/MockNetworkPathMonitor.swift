import Foundation
import AsyncExtensions

@testable import polkadot_app

final class MockNetworkPathMonitor: NetworkPathMonitoring {
    private let mutex = NSLock()
    private var continuation: AsyncStream<Bool>.Continuation?
    private let initialValue: Bool

    init(initial: Bool = true) {
        initialValue = initial
    }

    func pathStream() -> AnyAsyncSequence<Bool> {
        let (stream, continuation) = AsyncStream<Bool>.makeStream()

        mutex.lock()
        self.continuation = continuation
        mutex.unlock()

        // Yield the initial value immediately so the consumer sees it without waiting
        continuation.yield(initialValue)

        return stream.eraseToAnyAsyncSequence()
    }

    func send(_ isAvailable: Bool) {
        mutex.lock()
        defer { mutex.unlock() }
        continuation?.yield(isAvailable)
    }
}
