import AsyncExtensions
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
        public var refreshPolicy: ProductLoadPolicy
        public var clock: any Clock<Duration>

        public init(
            initialBackoff: Duration = .seconds(1),
            maxBackoff: Duration = .seconds(30),
            refreshPolicy: ProductLoadPolicy = .attempts(3),
            clock: any Clock<Duration> = ContinuousClock()
        ) {
            self.initialBackoff = initialBackoff
            self.maxBackoff = maxBackoff
            self.refreshPolicy = refreshPolicy
            self.clock = clock
        }
    }

    private let fetcher: ProductUpdateFetching
    private let cache: ProductVersionCaching
    private let environment: Environment
    private let timing: Timing
    private let logger: SDKLoggerProtocol
    private let updatesSubject = AsyncPassthroughSubject<ProductUpdate>()

    private var appliedVersions: [ProductId: ProductVersion] = [:]
    private var pendingByProduct: [ProductId: ProductUpdate] = [:]
    private var refreshTasks: [ProductId: Task<ProductUpdate?, Never>] = [:]

    public init(
        fetcher: ProductUpdateFetching,
        cache: ProductVersionCaching,
        environment: Environment,
        timing: Timing = Timing(),
        logger: SDKLoggerProtocol
    ) {
        self.fetcher = fetcher
        self.cache = cache
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
        if let cached = try? await cache.open(productId) {
            appliedVersions[cached.version.productId] = cached.version
            Task { _ = await self.refresh(productId) }
            return cached
        }

        try Task.checkCancellation()

        let fetched = try await retrying(productId, policy: policy, onPhase: onPhase) { [fetcher] in
            try await fetcher.fetch(productId, onPhase: onPhase)
        }
        await cache.apply(fetched.version)
        appliedVersions[fetched.version.productId] = fetched.version

        return fetched
    }

    public func refresh(_ productId: ProductId) async -> ProductUpdate? {
        let key = productId.lowercased()

        if let running = refreshTasks[key] {
            return await running.value
        }

        let task = Task { await performRefresh(productId) }
        refreshTasks[key] = task
        let update = await task.value
        refreshTasks[key] = nil

        return update
    }

    public func apply(_ update: ProductUpdate) async {
        await cache.apply(update.latest.version)
        appliedVersions[update.productId] = update.latest.version
        pendingByProduct[update.productId] = nil
    }

    public func pendingUpdates() -> [ProductUpdate] {
        Array(pendingByProduct.values)
    }

    public nonisolated func updates() -> AnyAsyncSequence<ProductUpdate> {
        updatesSubject.eraseToAnyAsyncSequence()
    }
}

private extension ProductContentLoader {
    func performRefresh(_ productId: ProductId) async -> ProductUpdate? {
        do {
            let fetched = try await retrying(productId, policy: timing.refreshPolicy) { [fetcher] in
                try await fetcher.fetch(productId)
            }

            return await publishIfChanged(fetched)
        } catch DotNsContractError.contentHashNotFound {
            await dropFromCache(productId)
            return nil
        } catch {
            logger.error("Product refresh failed for \(productId): \(error)")
            return nil
        }
    }

    func publishIfChanged(_ fetched: ProductContent) async -> ProductUpdate? {
        let productId = fetched.version.productId

        guard let applied = await appliedVersion(of: productId) else {
            await cache.apply(fetched.version)
            appliedVersions[productId] = fetched.version
            return nil
        }

        guard fetched.version != applied, fetched.version != pendingByProduct[productId]?.latest.version else {
            return nil
        }

        let update = ProductUpdate(previous: applied, latest: fetched)
        pendingByProduct[productId] = update
        updatesSubject.send(update)

        return update
    }

    func appliedVersion(of productId: ProductId) async -> ProductVersion? {
        if let applied = appliedVersions[productId] {
            return applied
        }

        return try? await cache.open(productId).version
    }

    func dropFromCache(_ productId: ProductId) async {
        guard let cached = try? await cache.open(productId) else { return }

        await cache.invalidate(cached.version)
        appliedVersions[cached.version.productId] = nil
        pendingByProduct[cached.version.productId] = nil
    }

    func retrying<T: Sendable>(
        _ productId: ProductId,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler = { _ in },
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
