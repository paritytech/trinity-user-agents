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
        try await withTimeout(.hangGuard) { [stream] in
            for await _ in stream {
                return
            }
            throw CancellationError()
        }
    }
}

struct CoalescingTaskTests {
    @Test func twoJoinersReceiveSameValueOperationRunsOnce() async throws {
        var executionCount = 0
        let coalescer = CoalescingTask<Int>()
        let started = Signal()
        let entering = Signal()
        let release = Signal()

        let task1 = Task {
            try await coalescer.run {
                executionCount += 1
                started.fire()
                try await release.wait()
                return executionCount
            }
        }

        try await started.wait()

        let task2 = Task {
            entering.fire()
            return try await coalescer.run {
                executionCount += 1
                return executionCount
            }
        }

        try await entering.wait()

        release.fire()

        let result1 = try await task1.value
        let result2 = try await task2.value

        #expect(result1 == 1) // First joiner gets count 1
        #expect(result2 == 1) // Second joiner also gets count 1 (same execution)
        #expect(executionCount == 1) // Operation ran exactly once
    }

    @Test func cancelledJoinerThrowsWhileOtherReceivesValue() async throws {
        var operationRunCount = 0
        let coalescer = CoalescingTask<Int>()
        let started = Signal()
        let entering = Signal()
        let release = Signal()

        let joiner1Task = Task {
            do {
                let val = try await coalescer.run {
                    operationRunCount += 1
                    started.fire()
                    try await release.wait()
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        try await started.wait()

        let joiner2Task = Task {
            entering.fire()
            do {
                let val = try await coalescer.run {
                    operationRunCount += 1
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        try await entering.wait()

        // Cancel joiner 1's task
        joiner1Task.cancel()

        // Resume the shared operation
        release.fire()

        let result1 = try await joiner1Task.value
        let result2 = try await joiner2Task.value

        // Joiner 1 should have thrown CancellationError
        if case let .failure(error) = result1 {
            #expect(error is CancellationError)
        } else {
            #expect(false, "Joiner 1 should have thrown CancellationError")
        }

        // Joiner 2 should have succeeded
        if case let .success(value) = result2 {
            #expect(value == 42)
        } else {
            #expect(false, "Joiner 2 should have succeeded")
        }

        // Operation should have run exactly once
        #expect(operationRunCount == 1)
    }

    @Test func afterCompletionSubsequentRunStartsFreshWork() async throws {
        var executionCount = 0
        let coalescer = CoalescingTask<Int>()

        // First run
        let result1 = try await coalescer.run {
            executionCount += 1
            return 100
        }
        #expect(result1 == 100)
        #expect(executionCount == 1)

        // Second run - should be fresh
        let result2 = try await coalescer.run {
            executionCount += 1
            return 200
        }
        #expect(result2 == 200)
        #expect(executionCount == 2)
    }

    @Test func joinerCancelledBeforeRegistrationStillThrows() async throws {
        let coalescer = CoalescingTask<Int>()
        let release = Signal()

        let joinerTask = Task {
            do {
                let val = try await coalescer.run {
                    try await release.wait()
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        // Cancel immediately before the task has a chance to register its continuation
        joinerTask.cancel()

        let result = try await joinerTask.value
        release.fire()

        // Should still throw CancellationError even though cancelled before registration
        if case let .failure(error) = result {
            #expect(error is CancellationError)
        } else {
            #expect(false, "Should have thrown CancellationError")
        }
    }
}
