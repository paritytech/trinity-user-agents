import Foundation

public actor ProductContentLoader {
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

    private let productResolver: ProductResolving
    private let dotNsResolver: DotNsResolverProtocol
    private let pathProvider: ProductNetworkPathProviding
    private let backgroundExecutor: ProductLoadBackgroundExecuting
    private let timing: Timing

    public init(
        productResolver: ProductResolving,
        dotNsResolver: DotNsResolverProtocol,
        pathProvider: ProductNetworkPathProviding,
        backgroundExecutor: ProductLoadBackgroundExecuting,
        timing: Timing = Timing()
    ) {
        self.productResolver = productResolver
        self.dotNsResolver = dotNsResolver
        self.pathProvider = pathProvider
        self.backgroundExecutor = backgroundExecutor
        self.timing = timing
    }
}

extension ProductContentLoader: ProductContentLoading {
    public func loadAppContent(of productId: ProductId, policy: ProductLoadPolicy) async throws -> URL {
        var failedAttempts = 0

        while true {
            do {
                return try await backgroundExecutor.execute { [self] in
                    try await resolveAppContent(of: productId)
                }
            } catch {
                try Task.checkCancellation()

                guard ProductLoadErrorClass(error) == .transient else {
                    throw error
                }

                guard try await isPathSatisfied() else {
                    try await waitForSatisfiedPath()
                    continue
                }

                failedAttempts += 1

                guard policy.allowsRetry(afterFailedAttempts: failedAttempts) else {
                    throw error
                }

                try await timing.clock.sleep(for: backoff(afterFailedAttempts: failedAttempts))
            }
        }
    }
}

private extension ProductContentLoader {
    func resolveAppContent(of productId: ProductId) async throws -> URL {
        let contentId = try await productResolver.resolve(productId).appContentId

        return try await dotNsResolver.resolveToLocalURL(dotNsName: contentId)
    }

    func isPathSatisfied() async throws -> Bool {
        try await pathProvider.pathStream().first { _ in true } ?? true
    }

    func waitForSatisfiedPath() async throws {
        _ = try await pathProvider.pathStream().first { $0 }
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
