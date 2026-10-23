import Foundation
import Operation_iOS
import StructuredConcurrencyTestSupport
import Testing
@testable import StructuredConcurrency

private struct Signal: Sendable {
    private let stream: AsyncStream<Void>
    private let continuation: AsyncStream<Void>.Continuation

    init() { (stream, continuation) = AsyncStream.makeStream() }

    func fire() { continuation.yield() }

    /// Returns once `fire()` has been called, failing rather than hanging if it never is.
    func wait() async throws {
        try await withTimeout(.hangGuard) {
            for await _ in stream {
                return
            }
            throw CancellationError()
        }
    }
}

private final class DeallocSentinel {
    static var key = 0

    private let signal: Signal

    init(_ signal: Signal) { self.signal = signal }

    deinit { signal.fire() }
}

struct AsyncTaskOperationTests {
    @Test func deliversResult() throws {
        let operation = AsyncTaskOperation<Int> { 42 }

        let queue = OperationQueue()
        queue.addOperations([operation], waitUntilFinished: true)

        #expect(try operation.extractNoCancellableResultData() == 42)
    }

    @Test func deliversError() {
        let expectedError = NSError(domain: "test", code: 1)

        let operation = AsyncTaskOperation<Int> {
            throw expectedError
        }

        let queue = OperationQueue()
        queue.addOperations([operation], waitUntilFinished: true)

        #expect(throws: expectedError) {
            try operation.extractNoCancellableResultData()
        }
    }

    @Test func cancelDuringExecutionCancelsTask() async throws {
        let started = Signal()
        let operation = AsyncTaskOperation<Int> {
            started.fire()
            try await Task.sleep(nanoseconds: 60_000_000_000)
            return 0
        }

        let queue = OperationQueue()
        queue.addOperation(operation)

        try await started.wait()
        operation.cancel()

        #expect(operation.isCancelled)
        #expect(throws: (any Error).self) {
            try operation.extractNoCancellableResultData()
        }
    }

    @Test func cancelPropagatesIntoTask() async throws {
        let entered = Signal()
        let cancelled = Signal()
        let operation = AsyncTaskOperation<Int> {
            await withTaskCancellationHandler {
                entered.fire()
                try? await Task.sleep(for: .seconds(60))
            } onCancel: {
                cancelled.fire()
            }
            return 0
        }

        let queue = OperationQueue()
        queue.addOperation(operation)

        try await entered.wait()
        operation.cancel()
        try await cancelled.wait()
    }

    @Test func operationDeallocates() async throws {
        weak var weakOperation: AsyncTaskOperation<Int>?
        let deallocated = Signal()

        autoreleasepool {
            let operation = AsyncTaskOperation<Int> { 42 }
            weakOperation = operation
            objc_setAssociatedObject(
                operation,
                &DeallocSentinel.key,
                DeallocSentinel(deallocated),
                .OBJC_ASSOCIATION_RETAIN
            )

            let queue = OperationQueue()
            queue.addOperations([operation], waitUntilFinished: true)
        }

        try await deallocated.wait()
        #expect(weakOperation == nil)
    }

    @Test func worksInDependencyChain() throws {
        let first = AsyncTaskOperation<Int> { 10 }

        let second = AsyncTaskOperation<Int> {
            let value = try first.extractNoCancellableResultData()
            return value * 2
        }

        second.addDependency(first)

        let queue = OperationQueue()
        queue.addOperations([first, second], waitUntilFinished: true)

        #expect(try second.extractNoCancellableResultData() == 20)
    }
}
