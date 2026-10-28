import AsyncExtensions
import Foundation

public struct NetworkFetchRetryStrategy: RetryStrategy {
    public enum Event: Sendable {
        case waitingForNetwork
        case networkRestored
        case attemptFailed(attempt: Int, error: Error)
        case willRetry(attempt: Int)
        case notRetryable(Error)
    }

    public struct Backoff: Sendable {
        private static let maxDoublings = 16

        public let initial: Duration
        public let max: Duration

        public init(initial: Duration = .seconds(1), max: Duration = .seconds(30)) {
            self.initial = initial
            self.max = max
        }

        func delay(afterFailedAttempts failedAttempts: Int) -> Duration {
            let doublings = min(failedAttempts - 1, Self.maxDoublings)

            return min(initial * (1 << doublings), max)
        }
    }

    public typealias EventHandler = @Sendable (Event) -> Void

    private let classifier: any RetryErrorClassifying
    private let maxAttempts: Int?
    private let backoff: Backoff
    private let clock: any Clock<Duration>
    private let pathProvider: any NetworkPathProviding
    private let onEvent: EventHandler

    private var failedAttempts = 0

    public init(
        classifier: any RetryErrorClassifying,
        maxAttempts: Int?,
        backoff: Backoff = Backoff(),
        clock: any Clock<Duration> = ContinuousClock(),
        pathProvider: any NetworkPathProviding,
        onEvent: @escaping EventHandler = { _ in }
    ) {
        self.classifier = classifier
        self.maxAttempts = maxAttempts
        self.backoff = backoff
        self.clock = clock
        self.pathProvider = pathProvider
        self.onEvent = onEvent
    }

    public mutating func decideOnFailure(_ error: Error) async throws {
        guard classifier.isRetryable(error) else {
            onEvent(.notRetryable(error))
            throw error
        }

        guard try await isPathSatisfied() else {
            onEvent(.waitingForNetwork)
            try await waitForSatisfiedPath()
            onEvent(.networkRestored)
            return
        }

        failedAttempts += 1
        onEvent(.attemptFailed(attempt: failedAttempts, error: error))

        guard maxAttempts.map({ failedAttempts < $0 }) ?? true else {
            throw error
        }

        onEvent(.willRetry(attempt: failedAttempts))
        try await clock.sleep(for: backoff.delay(afterFailedAttempts: failedAttempts))
    }
}

private extension NetworkFetchRetryStrategy {
    func isPathSatisfied() async throws -> Bool {
        try await pathProvider.pathStream().first { _ in true } ?? true
    }

    func waitForSatisfiedPath() async throws {
        _ = try await pathProvider.pathStream().first { $0 }
    }
}
