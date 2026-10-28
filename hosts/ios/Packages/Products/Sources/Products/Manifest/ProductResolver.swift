import TrUAPIHost
import Foundation
import SDKLogger

public enum ProductResolutionError: Error {
    case malformedManifest(ProductId)
    case productNotOnNetwork(ProductId)
}

public protocol ProductResolving: Sendable {
    func resolve(_ productId: ProductId) async throws -> ResolvedProduct
}

/// Resolves a product from its dotNS base name into metadata plus the executables it publishes.
///
/// Callers may pass a kind-prefixed subname; the canonical base is derived here.
public actor ProductResolver: ProductResolving {
    private let dotNsResolver: DotNsResolverProtocol
    private let hostProvider: ProductHostProviding
    private let parser: ProductManifestParsing

    // Successful resolutions only, so a transient read failure stays retryable.
    private var cache: [ProductId: ResolvedProduct] = [:]
    private var inFlight: [ProductId: Task<ResolvedProduct, Error>] = [:]

    public init(
        dotNsResolver: DotNsResolverProtocol,
        hostProvider: ProductHostProviding,
        logger: SDKLoggerProtocol,
        screening: PocketCardScreening
    ) {
        self.init(
            dotNsResolver: dotNsResolver,
            hostProvider: hostProvider,
            parser: ProductManifestParser(logger: logger, screening: screening)
        )
    }

    init(
        dotNsResolver: DotNsResolverProtocol,
        hostProvider: ProductHostProviding,
        parser: ProductManifestParsing
    ) {
        self.dotNsResolver = dotNsResolver
        self.hostProvider = hostProvider
        self.parser = parser
    }

    public func resolve(_ productId: ProductId) async throws -> ResolvedProduct {
        let unprefixed = try productIdOwner(productId: productId)

        guard let baseName = try await hostProvider.resolveHost(rawString: unprefixed.rawValue)?.toDotDomain() else {
            throw ProductResolutionError.productNotOnNetwork(unprefixed)
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

        let resolved = try await task.value
        cache[baseName] = resolved

        return resolved
    }
}

private extension ProductResolver {
    func resolveFromChain(_ baseName: ProductId) async throws -> ResolvedProduct {
        let rootText = try await dotNsResolver.getMetadataEntry(
            dotNsName: baseName.rawValue,
            key: ProductManifestRecords.rootKey
        )

        // No record is the legacy path: an app rooted at the base name, named after its domain.
        guard let rootText, !rootText.isEmpty else {
            return .legacy(id: baseName)
        }

        guard let root = parser.parseRoot(rootText) else {
            throw ProductResolutionError.malformedManifest(baseName)
        }

        let executables = try await resolveExecutables(baseName)

        return ResolvedProduct(
            id: baseName,
            displayName: root.displayName,
            description: root.description,
            icon: root.icon,
            executables: executables,
            hasManifest: true
        )
    }

    /// Every kind a surface on this host runs: an app screen, the widget an
    /// expanded Pocket card runs, and the product's worker.
    static let resolvedKinds: [ExecutableKind] = [.app, .widget, .worker]

    func resolveExecutables(_ baseName: ProductId) async throws -> ProductExecutables {
        try await withThrowingTaskGroup(of: ProductExecutable?.self) { group in
            for kind in Self.resolvedKinds {
                group.addTask { try await self.resolveExecutable(baseName, kind: kind) }
            }

            return try await group.reduce(into: ProductExecutables.empty) { executables, executable in
                guard let executable else { return }

                executables = executables.appending(executable)
            }
        }
    }

    /// An absent subname means the product does not provide that executable. A failed read is a
    /// different thing and propagates: swallowing it would cache a product as missing executables
    /// it actually publishes, and every surface would serve the base archive until the next launch.
    func resolveExecutable(_ baseName: ProductId, kind: ExecutableKind) async throws -> ProductExecutable? {
        let subname = try productIdExecutable(productId: baseName, kind: kind)

        let text = try await dotNsResolver.getMetadataEntry(
            dotNsName: subname.rawValue,
            key: ProductManifestRecords.executableKey
        )

        // Only the base-name owner can create these subnames, so ownership needs no re-check.
        return parser.parseExecutable(text, kind: kind, identifier: subname)
    }
}
