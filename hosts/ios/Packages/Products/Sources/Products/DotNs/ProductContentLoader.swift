import BackgroundExecution
import Foundation
import SDKLogger
import StructuredConcurrency

public actor ProductContentLoader {
    public struct Environment: Sendable {
        let pathProvider: NetworkPathProviding
        let backgroundExecutor: BackgroundExecuting

        public init(pathProvider: NetworkPathProviding, backgroundExecutor: BackgroundExecuting) {
            self.pathProvider = pathProvider
            self.backgroundExecutor = backgroundExecutor
        }
    }

    public struct Timing: Sendable {
        public var initialBackoff: Duration
        public var maxBackoff: Duration
        public var clock: any Clock<Duration>

        public init(
            initialBackoff: Duration = .seconds(1),
            maxBackoff: Duration = .seconds(30),
            clock: any Clock<Duration> = ContinuousClock()
        ) {
            self.initialBackoff = initialBackoff
            self.maxBackoff = maxBackoff
            self.clock = clock
        }
    }

    private let fetcher: ProductUpdateFetching
    private let environment: Environment
    private let timing: Timing
    private let logger: SDKLoggerProtocol

    public init(
        fetcher: ProductUpdateFetching,
        environment: Environment,
        timing: Timing = Timing(),
        logger: SDKLoggerProtocol
    ) {
        self.fetcher = fetcher
        self.environment = environment
        self.timing = timing
        self.logger = logger
    }
}

extension ProductContentLoader: ProductContentLoading {
    public func open(
        _ productId: ProductId,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent {
        try await retrying(productId, policy: policy, onPhase: onPhase) { [fetcher] in
            try await fetcher.fetch(productId, onPhase: onPhase)
        }
    }
}

private extension ProductContentLoader {
    func retrying<T: Sendable>(
        _ productId: ProductId,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let strategy = NetworkFetchRetryStrategy(
            classifier: ProductLoadErrorClassifier(),
            maxAttempts: policy.maxAttempts,
            backoff: NetworkFetchRetryStrategy.Backoff(initial: timing.initialBackoff, max: timing.maxBackoff),
            clock: timing.clock,
            pathProvider: environment.pathProvider,
            onEvent: { [logger] event in
                switch event {
                case .waitingForNetwork:
                    onPhase(.waitingForNetwork)
                case let .attemptFailed(attempt, error):
                    logger.warning("Product load attempt \(attempt) failed for \(productId): \(error)")
                case let .willRetry(attempt):
                    onPhase(.retrying(attempt: attempt))
                case .networkRestored,
                     .notRetryable:
                    break
                }
            }
        )

        return try await withRetry(strategy: strategy) { [backgroundExecutor = environment.backgroundExecutor] in
            try await backgroundExecutor.execute(operation)
        }
    }
}

private extension ProductLoadPolicy {
    var maxAttempts: Int? {
        switch self {
        case .untilCancelled:
            nil
        case let .attempts(limit):
            limit
        }
    }
}
