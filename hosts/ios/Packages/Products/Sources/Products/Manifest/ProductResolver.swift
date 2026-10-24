import Foundation
import SDKLogger

public enum ProductResolutionError: Error {
    case malformedManifest(ProductId)
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

    // Successful resolutions only, so a transient read failure stays retryable.
    private var cache: [ProductId: ProductManifest] = [:]
    private var inFlight: [ProductId: Task<ProductManifest, Error>] = [:]

    public init(
        metadataRepository: ProductMetadataRepositoryProtocol,
        hostProvider: ProductHostProviding,
        logger: SDKLoggerProtocol,
        screening: PocketCardScreening
    ) {
        self.init(
            metadataRepository: metadataRepository,
            hostProvider: hostProvider,
            parser: ProductManifestParser(logger: logger, screening: screening)
        )
    }

    init(
        metadataRepository: ProductMetadataRepositoryProtocol,
        hostProvider: ProductHostProviding,
        parser: ProductManifestParsing
    ) {
        self.metadataRepository = metadataRepository
        self.hostProvider = hostProvider
        self.parser = parser
    }

    public func resolveProductManifest(_ productId: ProductId) async throws -> ProductManifest {
        // Every form a caller might hold collapses here — a `<kind>.` subname, a `<root>.li`
        // mirror host, any casing — so no call site has to remember to canonicalise, and two
        // forms of one product cannot key the cache twice.
        //
        // dotNS is case-insensitive while name hashing is byte-exact.
        let unprefixed = ProductManifestRecords.baseName(of: productId.lowercased())

        // An id that is not a name on this network holds no records: a product installed by hand
        // through debug settings, or one whose TLD could not be read. Neither is worth a read.
        guard let baseName = try? await hostProvider.resolveHost(rawString: unprefixed)?.toDotDomain() else {
            // Strip the kind prefix but keep the case: a hand-installed id is keyed byte-exact by the
            // script storage and the chat extension id, so lowercasing it here loses the debug bot.
            return ProductManifest(product: .legacy(id: ProductManifestRecords.baseName(of: productId)), records: [:])
        }

        if let cached = cache[baseName] {
            return cached
        }

        // Concurrent callers share one read: a product opens from several surfaces at once.
        if let running = inFlight[baseName] {
            return try await running.value
        }

        let task = Task { [self] in try await resolveFromChain(baseName) }
        inFlight[baseName] = task

        defer { inFlight[baseName] = nil }

        let manifest = try await task.value
        cache[baseName] = manifest

        return manifest
    }
}

private struct ResolvedExecutable: Sendable {
    let executable: ProductExecutable?
    let recordKey: ProductVersion.RecordKey
    let record: DotNsSnapshotRecord
}

private extension ProductResolver {
    func resolveFromChain(_ baseName: ProductId) async throws -> ProductManifest {
        let rootKey = ProductVersion.RecordKey(name: baseName, key: ProductManifestRecords.rootKey)
        let rootText = try await metadataRepository.fetchMetadata(name: rootKey.name, key: rootKey.key)
        let rootRecords = [rootKey: DotNsSnapshotRecord(rootText)]

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
    static let resolvedKinds: [ExecutableKind] = ExecutableKind.allCases

    func resolveExecutables(
        _ baseName: ProductId
    ) async throws -> (ProductExecutables, [ProductVersion.RecordKey: DotNsSnapshotRecord]) {
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
        let subname = ProductManifestRecords.subname(base: baseName, kind: kind)
        let recordKey = ProductVersion.RecordKey(name: subname, key: ProductManifestRecords.executableKey)

        let text = try await metadataRepository.fetchMetadata(name: recordKey.name, key: recordKey.key)

        // Only the base-name owner can create these subnames, so ownership needs no re-check.
        return ResolvedExecutable(
            executable: parser.parseExecutable(text, kind: kind, identifier: subname),
            recordKey: recordKey,
            record: DotNsSnapshotRecord(text)
        )
    }
}
