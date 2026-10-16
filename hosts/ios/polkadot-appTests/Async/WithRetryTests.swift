import Testing
import StructuredConcurrency
import os

private struct TestError: Error {}

private final class AttemptCounter: @unchecked Sendable {
    private let lock = OSAllocatedUnfairLock(initialState: 0)

    func increment() -> Int {
        lock.withLock { $0 += 1; return $0 }
    }

    var value: Int {
        lock.withLock { $0 }
    }
}

@Suite("WithRetry operation")
struct WithRetryTests {
    @Test("a non-retryable error fails after one attempt")
    func nonRetryableErrorFailsAfterOneAttempt() async throws {
        let attempts = AttemptCounter()

        await #expect(throws: TestError.self) {
            try await withRetry(
                maxAttempts: 3,
                initialDelay: .zero,
                shouldRetry: { _ in false }
            ) {
                _ = attempts.increment()
                throw TestError()
            }
        }

        #expect(attempts.value == 1)
    }

    @Test("retries up to the attempt limit by default")
    func retriesUpToAttemptLimitByDefault() async throws {
        let attempts = AttemptCounter()

        await #expect(throws: TestError.self) {
            try await withRetry(
                maxAttempts: 3,
                initialDelay: .zero
            ) {
                _ = attempts.increment()
                throw TestError()
            }
        }

        #expect(attempts.value == 3)
    }

    @Test("returns the value once an attempt succeeds")
    func returnsValueOnceAttemptSucceeds() async throws {
        let attempts = AttemptCounter()

        let result = try await withRetry(
            maxAttempts: 3,
            initialDelay: .zero
        ) {
            if attempts.increment() < 3 {
                throw TestError()
            }
            return 42
        }

        #expect(result == 42)
        #expect(attempts.value == 3)
    }
}
