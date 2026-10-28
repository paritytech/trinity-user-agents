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
    private let cache: ProductContentCaching
    private let environment: Environment
    private let timing: Timing
    private let logger: SDKLoggerProtocol
    private let updatesSubject = AsyncPassthroughSubject<ProductContentUpdate>()

    private var refreshTasks: [ProductId: Task<[ProductContentUpdate], Never>] = [:]

    public init(
        fetcher: ProductUpdateFetching,
        cache: ProductContentCaching,
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
        kind: ExecutableKind,
        policy: ProductLoadPolicy,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> ProductContent {
        if let cached = await cachedContent(productId, kind: kind) {
            scheduleRefresh(productId)
            return cached
        }

        try Task.checkCancellation()

        let (manifest, content) = try await retrying(productId, policy: policy, onPhase: onPhase) { [fetcher] in
            onPhase(.resolving)

            let manifest = try await fetcher.fetchManifest(productId)

            guard let contentId = manifest.product.publishedContentId(for: kind) else {
                throw ProductContentError.executableNotPublished(kind)
            }

            let archive = try await fetcher.stage(contentId, onPhase: onPhase)

            return (manifest, ProductContent(resolved: manifest.product, kind: kind, archive: archive))
        }

        await cache.save(manifest, applying: content.contentId)
        await cache.apply(content.archive, record: nil, to: content.contentId)

        return content
    }

    public func openWorker(_ productId: ProductId) async throws -> ResolvedProduct {
        if let cached = try? await cache.manifest(productId) {
            await stageWorkerIfMissing(of: cached.product)
            scheduleRefresh(productId)
            return cached.product
        }

        let manifest = try await fetcher.fetchManifest(productId)
        await cache.save(manifest, applying: nil)
        await stageWorkerIfMissing(of: manifest.product)

        return manifest.product
    }

    public func refresh(_ productId: ProductId) async -> [ProductContentUpdate] {
        let key = productId.lowercased()

        if let running = refreshTasks[key] {
            return await running.value
        }

        let task = Task { await performRefresh(productId) }
        refreshTasks[key] = task
        let updates = await task.value
        refreshTasks[key] = nil

        return updates
    }

    public func apply(_ update: ProductContentUpdate) async {
        guard let archive = update.archive else {
            await cache.drop(update.contentId, record: update.record)
            return
        }

        await cache.apply(archive, record: update.record, to: update.contentId)
    }

    public nonisolated func updates() -> AnyAsyncSequence<ProductContentUpdate> {
        updatesSubject.eraseToAnyAsyncSequence()
    }
}

private struct FetchedContent: Sendable {
    let manifest: ProductManifest
    /// Every requested executable that is still published, by content id.
    let archives: [ProductId: DotNsResolvedContent]
}

private extension ResolvedProduct {
    /// The archive `kind` is served from, when the product publishes one for it. A legacy product
    /// serves every kind from its base name; a manifest product only the kinds it declares.
    func publishedContentId(for kind: ExecutableKind) -> ProductId? {
        let contentId = contentId(for: kind)
        let published = [appContentId, executables.widget?.identifier, executables.worker?.identifier]

        return published.contains(contentId) ? contentId : nil
    }
}

private extension ProductContentLoader {
    func cachedContent(_ productId: ProductId, kind: ExecutableKind) async -> ProductContent? {
        guard
            let manifest = try? await cache.manifest(productId),
            let contentId = manifest.product.publishedContentId(for: kind),
            let archive = await cache.appliedArchive(of: contentId)
        else {
            return nil
        }

        return ProductContent(resolved: manifest.product, kind: kind, archive: archive)
    }

    func scheduleRefresh(_ productId: ProductId) {
        Task { _ = await self.refresh(productId) }
    }

    func stageWorkerIfMissing(of resolved: ResolvedProduct) async {
        guard
            let worker = ProductWorkerSource.published(for: resolved, serving: .chat),
            await cache.appliedArchive(of: worker.contentId) == nil
        else {
            return
        }

        do {
            let archive = try await fetcher.stage(worker.contentId)
            await cache.apply(archive, record: nil, to: worker.contentId)
        } catch {
            logger.error("Failed to stage the worker archive \(worker.contentId): \(error)")
        }
    }

    /// Only executables with an applied archive are refreshed: those are the ones something on
    /// this device serves, and the rest are downloaded when first opened.
    func performRefresh(_ productId: ProductId) async -> [ProductContentUpdate] {
        guard let applied = try? await cache.manifest(productId) else { return [] }

        let served = await servedContentIds(of: applied.product)

        do {
            let latest = try await retrying(productId, policy: timing.refreshPolicy) { [fetcher] in
                try await Self.fetchLatest(productId, kinds: Array(served.keys), using: fetcher)
            }

            return await commit(latest, served: Set(served.values))
        } catch {
            logger.error("Product refresh failed for \(productId): \(error)")
            return []
        }
    }

    func servedContentIds(of resolved: ResolvedProduct) async -> [ExecutableKind: ProductId] {
        var served: [ExecutableKind: ProductId] = [:]

        for kind in ExecutableKind.allCases {
            guard
                let contentId = resolved.publishedContentId(for: kind),
                await cache.appliedArchive(of: contentId) != nil
            else {
                continue
            }

            served[kind] = contentId
        }

        return served
    }

    static func fetchLatest(
        _ productId: ProductId,
        kinds: [ExecutableKind],
        using fetcher: ProductUpdateFetching
    ) async throws -> FetchedContent {
        let manifest = try await fetcher.fetchManifest(productId)
        let contentIds = Set(kinds.compactMap { manifest.product.publishedContentId(for: $0) })

        var archives: [ProductId: DotNsResolvedContent] = [:]

        for contentId in contentIds {
            do {
                archives[contentId] = try await fetcher.stage(contentId)
            } catch DotNsContractError.contentHashNotFound {
                continue
            }
        }

        return FetchedContent(manifest: manifest, archives: archives)
    }

    /// An executable something serves changes only through an update that carries its record and
    /// archive from one release; the root record and executables nothing serves take effect now.
    func commit(_ latest: FetchedContent, served: Set<ProductId>) async -> [ProductContentUpdate] {
        await cache.save(latest.manifest, applying: nil)

        var updates: [ProductContentUpdate] = []

        for contentId in served {
            let record = latest.manifest.records[
                ProductManifest.RecordKey(name: contentId, key: ProductManifestRecords.executableKey)
            ]

            guard let archive = latest.archives[contentId] else {
                updates.append(ProductContentUpdate(contentId: contentId, archive: nil, record: record))
                continue
            }

            guard await cache.appliedArchive(of: contentId)?.contentHash != archive.contentHash else {
                await cache.apply(archive, record: record, to: contentId)
                continue
            }

            updates.append(ProductContentUpdate(contentId: contentId, archive: archive, record: record))
        }

        for (contentId, archive) in latest.archives where !served.contains(contentId) {
            await cache.apply(archive, record: nil, to: contentId)
        }

        updates.forEach(updatesSubject.send)

        return updates
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
