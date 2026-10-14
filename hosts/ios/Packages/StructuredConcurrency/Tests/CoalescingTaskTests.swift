import Testing
import Foundation
@testable import StructuredConcurrency

struct CoalescingTaskTests {
    @Test func twoJoinersReceiveSameValueOperationRunsOnce() async throws {
        var executionCount = 0
        let coalescer = CoalescingTask<Int>()

        var signalContinuation: AsyncStream<Void>.Continuation?
        let signal = AsyncStream<Void> { continuation in
            signalContinuation = continuation
        }

        let task1 = Task {
            try await coalescer.run {
                executionCount += 1
                var iterator = signal.makeAsyncIterator()
                _ = try await iterator.next()
                return executionCount
            }
        }

        // Give first task time to start
        try await Task.sleep(nanoseconds: 50_000_000) // 0.05s

        let task2 = Task {
            try await coalescer.run {
                executionCount += 1
                return executionCount
            }
        }

        // Give tasks time to both register
        try await Task.sleep(nanoseconds: 50_000_000) // 0.05s

        // Resume the operation
        signalContinuation?.yield()

        let result1 = try await task1.value
        let result2 = try await task2.value

        #expect(result1 == 1) // First joiner gets count 1
        #expect(result2 == 1) // Second joiner also gets count 1 (same execution)
        #expect(executionCount == 1) // Operation ran exactly once
    }

    @Test func cancelledJoinerThrowsWhileOtherReceivesValue() async throws {
        var operationRunCount = 0
        let coalescer = CoalescingTask<Int>()

        var signalContinuation: AsyncStream<Void>.Continuation?
        let signal = AsyncStream<Void> { continuation in
            signalContinuation = continuation
        }

        let joiner1Task = Task {
            do {
                let val = try await coalescer.run {
                    operationRunCount += 1
                    var iterator = signal.makeAsyncIterator()
                    _ = try await iterator.next()
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        // Give joiner 1 time to register
        try await Task.sleep(nanoseconds: 100_000_000) // 0.1s

        let joiner2Task = Task {
            do {
                let val = try await coalescer.run {
                    operationRunCount += 1
                    var iterator = signal.makeAsyncIterator()
                    _ = try await iterator.next()
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        // Give joiner 2 time to register
        try await Task.sleep(nanoseconds: 50_000_000) // 0.05s

        // Cancel joiner 1's task
        joiner1Task.cancel()

        // Let cancellation propagate
        try await Task.sleep(nanoseconds: 50_000_000) // 0.05s

        // Resume the shared operation
        signalContinuation?.yield()

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

        // Give first task time to clean up
        try await Task.sleep(nanoseconds: 100_000_000) // 0.1s

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

        let joinerTask = Task {
            do {
                let val = try await coalescer.run {
                    try await Task.sleep(nanoseconds: 500_000_000) // 0.5s
                    return 42
                }
                return Result<Int, Error>.success(val)
            } catch {
                return Result<Int, Error>.failure(error)
            }
        }

        // Cancel immediately before the task has a chance to register its continuation
        try await Task.sleep(nanoseconds: 1_000_000) // 1ms - very short
        joinerTask.cancel()

        let result = try await joinerTask.value

        // Should still throw CancellationError even though cancelled before registration
        if case let .failure(error) = result {
            #expect(error is CancellationError)
        } else {
            #expect(false, "Should have thrown CancellationError")
        }
    }
}
