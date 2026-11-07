import TrUAPIHost
import Foundation
import IssueMonitoring
import SDKLogger

public enum ProductResolutionError: Error {
    case malformedManifest(ProductId)
    case productNotOnNetwork(ProductId)
    case missingExecutable(ProductId, ExecutableKind)
}

public protocol ProductResolving: Sendable {
    func resolveProductManifest(_ productId: ProductId) async throws -> ProductManifest
}

public extension ProductResolving {
    func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        try await resolveProductManifest(productId).product
    }
}

/// Resolves a product from its dotNS base name into metadata plus the executables it publishes.
///
/// Callers may pass a kind-prefixed subname; the canonical base is derived here.
public actor ProductResolver: ProductResolving {
    private let metadataRepository: ProductMetadataRepositoryProtocol
    private let hostProvider: ProductHostProviding
    private let parser: ProductManifestParsing
    private let telemetry: HostLoadTelemetry

    // Successful resolutions only, so a transient read failure stays retryable.
    private var cache: [ProductId: ProductManifest] = [:]
    private var inFlight: [ProductId: (read: Task<ProductManifest, Error>, context: HostLoadContext?)] = [:]

    public init(
        metadataRepository: ProductMetadataRepositoryProtocol,
        hostProvider: ProductHostProviding,
        logger: SDKLoggerProtocol,
        screening: PocketCardScreening,
        telemetry: HostLoadTelemetry = .disabled
    ) {
        self.init(
            metadataRepository: metadataRepository,
            hostProvider: hostProvider,
            parser: ProductManifestParser(logger: logger, screening: screening),
            telemetry: telemetry
        )
    }

    init(
        metadataRepository: ProductMetadataRepositoryProtocol,
        hostProvider: ProductHostProviding,
        parser: ProductManifestParsing,
        telemetry: HostLoadTelemetry = .disabled
    ) {
        self.metadataRepository = metadataRepository
        self.hostProvider = hostProvider
        self.parser = parser
        self.telemetry = telemetry
    }

    public func resolveProductManifest(_ productId: ProductId) async throws -> ProductManifest {
        let startedAt = telemetry.loadStepStart()
        let baseName: ProductId

        do {
            baseName = try await resolveBaseName(of: productId)
        } catch {
            if !(error is CancellationError) {
                // The same form `ok` and `cache_hit` report: the product's own name, without a prefix.
                let reported = (try? productIdOwner(productId: productId)) ?? productId
                reportResolved(reported, outcome: .error, startedAt: startedAt)
            }
            throw error
        }

        if let cached = cache[baseName] {
            reportResolved(baseName, outcome: .cacheHit, startedAt: startedAt)
            return cached
        }

        // Concurrent callers share one read: a product opens from several surfaces at once.
        if let running = inFlight[baseName] {
            return try await join(running.read, of: baseName, startedBy: running.context, startedAt: startedAt)
        }

        let task = Task { [self] in try await resolveFromChainReporting(baseName, startedAt: startedAt) }
        inFlight[baseName] = (task, telemetry.context)

        defer { inFlight[baseName] = nil }

        let manifest = try await task.value
        cache[baseName] = manifest

        return manifest
    }
}

private struct ResolvedExecutable: Sendable {
    let executable: ProductExecutable?
    let recordKey: ProductManifest.RecordKey
    let record: ProductManifest.Record
}

private extension ProductResolver {
    static let resolvedStep = "host.product.resolved"

    func resolveBaseName(of productId: ProductId) async throws -> ProductId {
        let unprefixed = try productIdOwner(productId: productId)

        guard let baseName = try await hostProvider.resolveHost(rawString: unprefixed.rawValue)?.toDotDomain() else {
            throw ProductResolutionError.productNotOnNetwork(unprefixed)
        }

        return baseName
    }

    /// The shared read reports under the context that started it, so a caller from another load (a
    /// tap joining a prewarm's read) reports its own wait.
    func join(
        _ read: Task<ProductManifest, Error>,
        of baseName: ProductId,
        startedBy owner: HostLoadContext?,
        startedAt: Int?
    ) async throws -> ProductManifest {
        let ownWait = telemetry.context == owner ? nil : startedAt

        do {
            let manifest = try await read.value
            reportResolved(baseName, outcome: .ok, startedAt: ownWait)
            return manifest
        } catch {
            if !(error is CancellationError) {
                reportResolved(baseName, outcome: .error, startedAt: ownWait)
            }
            throw error
        }
    }

    func resolveFromChainReporting(_ baseName: ProductId, startedAt: Int?) async throws -> ProductManifest {
        do {
            let manifest = try await resolveFromChain(baseName)
            reportResolved(baseName, outcome: .ok, startedAt: startedAt)
            return manifest
        } catch {
            if !(error is CancellationError) {
                reportResolved(baseName, outcome: .error, startedAt: startedAt)
            }
            throw error
        }
    }

    func reportResolved(_ baseName: ProductId, outcome: HostEventOutcome, startedAt: Int?) {
        guard
            let startedAt,
            let context = telemetry.context,
            telemetry.shouldReport(Self.resolvedStep, productID: baseName.rawValue, outcome: outcome)
        else { return }

        telemetry.record { origin, now in
            .productResolved(
                productID: baseName.rawValue,
                outcome: outcome,
                trigger: context.trigger,
                durationMilliseconds: now - startedAt,
                hostSessionID: context.sessionID,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }

    func resolveFromChain(_ baseName: ProductId) async throws -> ProductManifest {
        let rootKey = ProductManifest.RecordKey(name: baseName, key: ProductManifestRecords.rootKey)
        let rootText = try await metadataRepository.fetchMetadata(name: rootKey.name.rawValue, key: rootKey.key)
        let rootRecords = [rootKey: ProductManifest.Record(rootText)]

        // No record is the legacy path: an app rooted at the base name, named after its domain.
        guard let rootText, !rootText.isEmpty else {
            return ProductManifest(product: .legacy(id: baseName), records: rootRecords)
        }

        guard let root = parser.parseRoot(rootText) else {
            throw ProductResolutionError.malformedManifest(baseName)
        }

        let (executables, executableRecords) = try await resolveExecutables(baseName)

        let product = ResolvedProduct(
            id: baseName,
            displayName: root.displayName,
            description: root.description,
            icon: root.icon,
            executables: executables,
            hasManifest: true
        )

        return ProductManifest(product: product, records: rootRecords.merging(executableRecords) { first, _ in first })
    }

    /// Every kind a surface on this host runs: an app screen, the widget an
    /// expanded Pocket card runs, and the product's worker.
    static let resolvedKinds: [ExecutableKind] = [.app, .widget, .worker]

    func resolveExecutables(
        _ baseName: ProductId
    ) async throws -> (ProductExecutables, [ProductManifest.RecordKey: ProductManifest.Record]) {
        try await withThrowingTaskGroup(of: ResolvedExecutable.self) { group in
            for kind in Self.resolvedKinds {
                group.addTask { try await self.resolveExecutable(baseName, kind: kind) }
            }

            return try await group.reduce(into: (ProductExecutables.empty, [:])) { result, resolved in
                result.1[resolved.recordKey] = resolved.record

                guard let executable = resolved.executable else { return }

                result.0 = result.0.appending(executable)
            }
        }
    }

    /// An absent subname means the product does not provide that executable. A failed read is a
    /// different thing and propagates: swallowing it would cache a product as missing executables
    /// it actually publishes, and every surface would serve the base archive until the next launch.
    func resolveExecutable(_ baseName: ProductId, kind: ExecutableKind) async throws -> ResolvedExecutable {
        let subname = try productIdExecutable(productId: baseName, kind: kind)
        let recordKey = ProductManifest.RecordKey(name: subname, key: ProductManifestRecords.executableKey)

        let text = try await metadataRepository.fetchMetadata(name: recordKey.name.rawValue, key: recordKey.key)

        // Only the base-name owner can create these subnames, so ownership needs no re-check.
        return ResolvedExecutable(
            executable: parser.parseExecutable(text, kind: kind, identifier: subname),
            recordKey: recordKey,
            record: ProductManifest.Record(text)
        )
    }
}
