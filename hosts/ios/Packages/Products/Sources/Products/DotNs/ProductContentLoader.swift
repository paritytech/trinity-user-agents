import AsyncExtensions
import BackgroundExecution
import Foundation
import SDKLogger

public actor ProductContentLoader {
    public struct Environment: Sendable {
        let pathProvider: ProductNetworkPathProviding
        let backgroundExecutor: BackgroundExecuting
        let diagnostics: ProductLoadDiagnostics

        public init(
            pathProvider: ProductNetworkPathProviding,
            backgroundExecutor: BackgroundExecuting,
            diagnostics: ProductLoadDiagnostics = ProductLoadDiagnostics()
        ) {
            self.pathProvider = pathProvider
            self.backgroundExecutor = backgroundExecutor
            self.diagnostics = diagnostics
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

    private enum Constants {
        static let maxBackoffDoublings = 16
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

        let fetched = try await retrying(productId, policy: policy, onPhase: onPhase, diagnosing: true) { [fetcher] in
            try await fetcher.fetch(productId, onPhase: onPhase)
        }
        await cache.apply(fetched.version)
        appliedVersions[fetched.version.productId] = fetched.version

        return ProductContent(
            resolved: fetched.resolved,
            appContent: fetched.appContent,
            version: fetched.version,
            source: .network
        )
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

    public func removeSupersededContent(keeping servedHashes: Set<String>) async {
        let pendingHashes = pendingByProduct.values.flatMap(\.latest.version.contentHashes.values)
        await cache.removeSupersededContent(keeping: servedHashes.union(pendingHashes))
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

    func publishIfChanged(_ fetched: FetchedProductVersion) async -> ProductUpdate? {
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
        onPhase: ProductLoadPhaseHandler = { _ in },
        diagnosing: Bool = false,
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let diagnostics = environment.diagnostics
        var attempts = await ProductLoadAttempts(productId: productId, startedAt: diagnostics.dateProvider.read())

        while true {
            do {
                return try await environment.backgroundExecutor.execute(operation)
            } catch {
                try Task.checkCancellation()

                guard ProductLoadErrorClass(error) == .transient else {
                    if diagnosing { diagnostics.reportUnavailable(error) }
                    throw error
                }

                guard try await isPathSatisfied() else {
                    onPhase(.waitingForNetwork)
                    try await waitForSatisfiedPath()
                    await attempts.restart(at: diagnostics.dateProvider.read())
                    continue
                }

                attempts.recordFailure()
                logger.warning("Product load attempt \(attempts.failed) failed for \(productId): \(error)")

                if diagnosing {
                    await reportStallIfNeeded(&attempts, error: error)
                }

                guard policy.allowsRetry(afterFailedAttempts: attempts.failed) else {
                    throw error
                }

                onPhase(.retrying(attempt: attempts.failed))
                try await timing.clock.sleep(for: backoff(afterFailedAttempts: attempts.failed))
            }
        }
    }

    func reportStallIfNeeded(_ attempts: inout ProductLoadAttempts, error: Error) async {
        let diagnostics = environment.diagnostics
        let now = await diagnostics.dateProvider.read()

        guard attempts.claimStallReport(at: now, thresholds: diagnostics.thresholds) else { return }

        diagnostics.reportStall(attempts, error: error, at: now)
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
