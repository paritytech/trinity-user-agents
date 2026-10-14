import Testing
import Foundation
import Operation_iOS
@testable import StructuredConcurrency

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
        let operation = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 60_000_000_000)
            return 0
        }

        let queue = OperationQueue()
        queue.addOperation(operation)

        try await Task.sleep(nanoseconds: 100_000_000)
        operation.cancel()
        try await Task.sleep(nanoseconds: 200_000_000)

        #expect(operation.isCancelled)
        #expect(throws: (any Error).self) {
            try operation.extractNoCancellableResultData()
        }
    }

    @Test func cancelPropagatesIntoTask() async throws {
        await confirmation("task cancellation handler called") { confirm in
            let operation = AsyncTaskOperation<Int> {
                await withTaskCancellationHandler {
                    try? await Task.sleep(nanoseconds: 60_000_000_000)
                } onCancel: {
                    confirm()
                }
                return 0
            }

            let queue = OperationQueue()
            queue.addOperation(operation)

            try? await Task.sleep(nanoseconds: 100_000_000)
            operation.cancel()
        }
    }

    @Test func operationDeallocates() async throws {
        weak var weakOperation: AsyncTaskOperation<Int>?

        autoreleasepool {
            let operation = AsyncTaskOperation<Int> { 42 }
            weakOperation = operation

            let queue = OperationQueue()
            queue.addOperations([operation], waitUntilFinished: true)
        }

        try await Task.sleep(nanoseconds: 100_000_000)
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
