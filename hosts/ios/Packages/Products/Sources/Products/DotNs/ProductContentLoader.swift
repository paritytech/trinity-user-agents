import AsyncExtensions
import BackgroundExecution
import Foundation
import SDKLogger

public actor ProductContentLoader {
    public struct Environment: Sendable {
        let pathProvider: ProductNetworkPathProviding
        let backgroundExecutor: BackgroundExecuting

        public init(pathProvider: ProductNetworkPathProviding, backgroundExecutor: BackgroundExecuting) {
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

    private enum Constants {
        static let maxBackoffDoublings = 16
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
        onPhase: ProductLoadPhaseHandler,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        var failedAttempts = 0

        while true {
            do {
                return try await environment.backgroundExecutor.execute(operation)
            } catch {
                try Task.checkCancellation()

                guard ProductLoadErrorClass(error) == .transient else {
                    throw error
                }

                guard try await isPathSatisfied() else {
                    onPhase(.waitingForNetwork)
                    try await waitForSatisfiedPath()
                    continue
                }

                failedAttempts += 1
                logger.warning("Product load attempt \(failedAttempts) failed for \(productId): \(error)")

                guard policy.allowsRetry(afterFailedAttempts: failedAttempts) else {
                    throw error
                }

                onPhase(.retrying(attempt: failedAttempts))
                try await timing.clock.sleep(for: backoff(afterFailedAttempts: failedAttempts))
            }
        }
    }

    func isPathSatisfied() async throws -> Bool {
        try await environment.pathProvider.pathStream().first { _ in true } ?? true
    }

    func waitForSatisfiedPath() async throws {
        _ = try await environment.pathProvider.pathStream().first { $0 }
    }

    func backoff(afterFailedAttempts failedAttempts: Int) -> Duration {
        let doublings = min(failedAttempts - 1, Constants.maxBackoffDoublings)

        return min(timing.initialBackoff * (1 << doublings), timing.maxBackoff)
    }
}

private extension ProductLoadPolicy {
    func allowsRetry(afterFailedAttempts failedAttempts: Int) -> Bool {
        switch self {
        case .untilCancelled:
            true
        case let .attempts(limit):
            failedAttempts < limit
        }
    }
}
