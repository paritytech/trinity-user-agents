import AsyncExtensions
import Foundation
import FoundationExt
import os
import StructuredConcurrencyTestSupport
import Testing
@testable import StructuredConcurrency

struct NetworkFetchRetryStrategyTests {
    private let clock = ManualClock()

    @Test func retriesWithCappedExponentialBackoffUntilTheOperationSucceeds() async throws {
        let operation = FailingOperation(failures: Array(repeating: URLError(.timedOut), count: 3))
        let strategy = makeStrategy(maxAttempts: nil)

        let run = Task { try await withRetry(strategy: strategy) { try operation.run() } }

        for delay in [1, 2, 2] {
            await clock.resumeSleep(for: .seconds(delay))
        }

        #expect(try await run.value == 4)
    }

    @Test func stopsWithTheOriginalErrorOnceTheAttemptsAreSpent() async throws {
        let operation = FailingOperation(failures: Array(repeating: URLError(.timedOut), count: 3))
        let strategy = makeStrategy(maxAttempts: 2)

        let run = Task { try await withRetry(strategy: strategy) { try operation.run() } }

        await clock.resumeSleep(for: .seconds(1))

        await #expect(throws: URLError(.timedOut)) { try await run.value }
        #expect(operation.attempts == 2)
    }

    @Test func failsAtOnceOnAnErrorTheClassifierRejects() async throws {
        let operation = FailingOperation(failures: [HTTPResponseError(statusCode: 404)])

        await #expect(throws: HTTPResponseError.self) {
            try await withRetry(strategy: makeStrategy(maxAttempts: nil)) { try operation.run() }
        }

        #expect(operation.attempts == 1)
    }
}

private extension NetworkFetchRetryStrategyTests {
    func makeStrategy(maxAttempts: Int?) -> NetworkFetchRetryStrategy {
        NetworkFetchRetryStrategy(
            classifier: NetworkErrorClassifier(),
            maxAttempts: maxAttempts,
            backoff: NetworkFetchRetryStrategy.Backoff(initial: .seconds(1), max: .seconds(2)),
            clock: clock,
            pathProvider: SatisfiedNetworkPath()
        )
    }
}

private struct SatisfiedNetworkPath: NetworkPathProviding {
    func pathStream() -> AnyAsyncSequence<Bool> {
        AsyncCurrentValueSubject(true).eraseToAnyAsyncSequence()
    }
}

private final class FailingOperation: Sendable {
    private struct State {
        var failures: [Error]
        var attempts = 0
    }

    private let state: OSAllocatedUnfairLock<State>

    init(failures: [Error]) {
        state = OSAllocatedUnfairLock(uncheckedState: State(failures: failures))
    }

    var attempts: Int {
        state.withLock { $0.attempts }
    }

    func run() throws -> Int {
        try state.withLockUnchecked { state in
            state.attempts += 1

            guard state.failures.isEmpty else {
                throw state.failures.removeFirst()
            }

            return state.attempts
        }
    }
}
