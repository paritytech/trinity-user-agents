import Testing
import Foundation
import Operation_iOS
@testable import StructuredConcurrency

struct OperationsConcurrencyTests {
    // MARK: - Basics

    @Test func returnsResult() async throws {
        let operation = AsyncTaskOperation<Int> { 42 }

        let result = try await operation.asyncExecute()

        #expect(result == 42)
        #expect(operation.isFinished)
    }

    @Test func throwsOperationError() async throws {
        let operation = AsyncTaskOperation<Int> {
            throw TestError.boom
        }

        await #expect(throws: TestError.boom) {
            _ = try await operation.asyncExecute()
        }
    }

    @Test func executesCompoundWrapperWithDependency() async throws {
        let dependency = AsyncTaskOperation<Int> { 10 }
        let target = AsyncTaskOperation<Int> {
            try dependency.extractNoCancellableResultData() * 2
        }
        target.addDependency(dependency)

        let wrapper = CompoundOperationWrapper(
            targetOperation: target,
            dependencies: [dependency]
        )

        let result = try await wrapper.asyncExecute()

        #expect(result == 20)
        #expect(dependency.isFinished)
        #expect(target.isFinished)
    }

    // MARK: - Pre-resolved operations (`.finished` branch)

    @Test func returnsAlreadyComputedResult() async throws {
        let operation = BaseOperation<Int>.createWithResult(99)

        let result = try await operation.asyncExecute()

        #expect(result == 99)
    }

    @Test func throwsAlreadyStoredError() async throws {
        let operation = BaseOperation<Int>.createWithError(TestError.boom)

        await #expect(throws: TestError.boom) {
            _ = try await operation.asyncExecute()
        }
    }

    // MARK: - Cancellation

    @Test func cancelDuringExecutionThrowsAndMarksOperationCancelled() async throws {
        let operation = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 10_000_000_000)
            return 42
        }

        let task = Task<Int, Error> {
            try await operation.asyncExecute()
        }

        // Give the task time to enter `asyncExecute` and enqueue the operation.
        try await Task.sleep(nanoseconds: 100_000_000)
        task.cancel()

        await #expect(throws: (any Error).self) {
            _ = try await task.value
        }

        #expect(operation.isCancelled)
    }

    @Test func cancelledTaskShortCircuitsBeforeEnqueue() async throws {
        // Long-running operation. If `asyncExecute` actually enqueued it, the
        // test would either hang or take ages.
        let operation = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 10_000_000_000)
            return 42
        }

        let task = Task<Int, Error> {
            // Spin until cancellation is observable so that `asyncExecute`
            // sees `Task.isCancelled == true` on the very first check.
            while !Task.isCancelled {
                await Task.yield()
            }
            return try await operation.asyncExecute()
        }
        task.cancel()

        await #expect(throws: CancellationError.self) {
            _ = try await task.value
        }

        // The wrapper's `cancel()` was called — the underlying op is marked cancelled.
        #expect(operation.isCancelled)
    }

    @Test func cancelledTaskWinsOverFinishedOperation() async throws {
        // Operation is already finished, but the task is cancelled — the
        // contract of Swift Concurrency says we honour cancellation first.
        let operation = BaseOperation<Int>.createWithResult(42)

        let task = Task<Int, Error> {
            while !Task.isCancelled {
                await Task.yield()
            }
            return try await operation.asyncExecute()
        }
        task.cancel()

        await #expect(throws: CancellationError.self) {
            _ = try await task.value
        }
    }

    // MARK: - Concurrency

    @Test func concurrentExecutionsAreIndependent() async throws {
        let op1 = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 100_000_000)
            return 1
        }
        let op2 = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 100_000_000)
            return 2
        }

        async let r1 = op1.asyncExecute()
        async let r2 = op2.asyncExecute()

        let (a, b) = try await (r1, r2)

        #expect(a == 1)
        #expect(b == 2)
    }

    @Test func cancellingOneTaskDoesNotAffectOther() async throws {
        let slow = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 5_000_000_000)
            return 1
        }
        let fast = AsyncTaskOperation<Int> {
            try await Task.sleep(nanoseconds: 200_000_000)
            return 2
        }

        let slowTask = Task<Int, Error> { try await slow.asyncExecute() }
        let fastTask = Task<Int, Error> { try await fast.asyncExecute() }

        try await Task.sleep(nanoseconds: 50_000_000)
        slowTask.cancel()

        await #expect(throws: (any Error).self) {
            _ = try await slowTask.value
        }

        let fastResult = try await fastTask.value
        #expect(fastResult == 2)
        #expect(slow.isCancelled)
        #expect(fast.isFinished)
        #expect(!fast.isCancelled)
    }

    // MARK: - Idempotency under racing cancel + completion

    @Test func rapidCancelDoesNotCrashOrHang() async throws {
        // Many short operations cancelled with no warm-up time. Exercises the
        // race between `completionBlock` resuming the guard and `onCancel`
        // resuming it via `CancellationError`. The guard must keep resume
        // exactly-once or this would either crash (double resume) or hang
        // (no resume).
        for _ in 0 ..< 200 {
            let operation = AsyncTaskOperation<Int> { 7 }
            let task = Task<Int, Error> { try await operation.asyncExecute() }
            task.cancel()
            _ = try? await task.value
        }
    }
}

// MARK: - Test helpers

private enum TestError: Error, Equatable {
    case boom
}
