import Foundation
import SDKLogger

public struct FetchedProductVersion: Equatable, Sendable {
    public let version: ProductVersion
    public let resolved: ResolvedProduct
    public let appContent: DotNsResolvedContent
    public let contents: [ProductId: DotNsResolvedContent]

    public init(
        version: ProductVersion,
        resolved: ResolvedProduct,
        appContent: DotNsResolvedContent,
        contents: [ProductId: DotNsResolvedContent]
    ) {
        self.version = version
        self.resolved = resolved
        self.appContent = appContent
        self.contents = contents
    }

    public func content(for kind: ExecutableKind) -> DotNsResolvedContent? {
        contents[resolved.contentId(for: kind)]
    }
}

public enum ProductUpdateFetchError: Error, Equatable {
    case tldUnavailable
}

public protocol ProductUpdateFetching: Sendable {
    func fetch(_ productId: ProductId, onPhase: @escaping ProductLoadPhaseHandler) async throws -> FetchedProductVersion
}

public extension ProductUpdateFetching {
    func fetch(_ productId: ProductId) async throws -> FetchedProductVersion {
        try await fetch(productId, onPhase: { _ in })
    }
}

public actor ProductUpdateFetcher {
    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let storage: DotNsLocalStorage
    private let hostProvider: ProductHostProviding
    private let resolverFactory: ProductResolverFactory

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: DotNsLocalStorage,
        hostProvider: ProductHostProviding,
        resolverFactory: ProductResolverFactory
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.storage = storage
        self.hostProvider = hostProvider
        self.resolverFactory = resolverFactory
    }
}

extension ProductUpdateFetcher: ProductUpdateFetching {
    public func fetch(
        _ productId: ProductId,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> FetchedProductVersion {
        do {
            _ = try await hostProvider.resolveHost(rawString: productId.lowercased())
        } catch {
            throw ProductUpdateFetchError.tldUnavailable
        }

        onPhase(.resolving)

        let dotNsResolver = makeDotNsResolver()

        let manifest = try await resolverFactory.makeResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider
        ).resolveProductManifest(productId)
        let resolved = manifest.product

        let appContent = try await dotNsResolver.stageContent(dotNsName: resolved.appContentId, onPhase: onPhase)
        var contents = [resolved.appContentId: appContent]

        for contentId in optionalContentIds(of: resolved) where contents[contentId] == nil {
            if let content = try await stageOptional(contentId, of: resolved.id, using: dotNsResolver) {
                contents[contentId] = content
            }
        }

        return FetchedProductVersion(
            version: ProductVersion(
                productId: resolved.id,
                records: manifest.records,
                contentHashes: contents.mapValues(\.contentHash)
            ),
            resolved: resolved,
            appContent: appContent,
            contents: contents
        )
    }
}

private extension ProductUpdateFetcher {
    func makeDotNsResolver() -> DotNsResolver {
        DotNsResolver(
            contractApi: contractApi,
            carFetcher: carFetcher,
            contentStorage: storage.content,
            contentHashCache: storage.contentHashes
        )
    }

    func optionalContentIds(of resolved: ResolvedProduct) -> [ProductId] {
        [resolved.executables.widget?.identifier, resolved.executables.worker?.identifier].compactMap { $0 }
    }

    func stageOptional(
        _ contentId: ProductId,
        of productId: ProductId,
        using dotNsResolver: DotNsResolver
    ) async throws -> DotNsResolvedContent? {
        do {
            return try await dotNsResolver.stageContent(dotNsName: contentId)
        } catch DotNsContractError.contentHashNotFound {
            resolverFactory.logger.warning("Product \(productId) declares \(contentId) with no published content")
            return nil
        }
    }
}
