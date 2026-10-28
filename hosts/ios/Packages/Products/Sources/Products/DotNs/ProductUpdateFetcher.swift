import Foundation
import SDKLogger

public enum ProductUpdateFetchError: Error, Equatable {
    case tldUnavailable
}

public protocol ProductUpdateFetching: Sendable {
    func fetch(_ productId: ProductId, onPhase: @escaping ProductLoadPhaseHandler) async throws -> ProductContent
}

public extension ProductUpdateFetching {
    func fetch(_ productId: ProductId) async throws -> ProductContent {
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
    ) async throws -> ProductContent {
        do {
            _ = try await hostProvider.resolveHost(rawString: productId.lowercased())
        } catch {
            throw ProductUpdateFetchError.tldUnavailable
        }

        onPhase(.resolving)

        let dotNsResolver = makeDotNsResolver()

        let resolved = try await resolverFactory.makeResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider
        ).resolve(productId)

        let appContent = try await dotNsResolver.stageContent(dotNsName: resolved.appContentId, onPhase: onPhase)
        var contents = [resolved.appContentId: appContent]

        for contentId in optionalContentIds(of: resolved) where contents[contentId] == nil {
            if let content = try await resolveOptional(contentId, of: resolved.id, using: dotNsResolver) {
                contents[contentId] = content
            }
        }

        for (contentId, content) in contents {
            storage.contentHashes.putContentHash(name: contentId, hash: content.contentHash)
        }

        return ProductContent(resolved: resolved, contents: contents, appContent: appContent)
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
        ExecutableKind.allCases.map(resolved.contentId(for:))
    }

    func resolveOptional(
        _ contentId: ProductId,
        of productId: ProductId,
        using dotNsResolver: DotNsResolver
    ) async throws -> DotNsResolvedContent? {
        do {
            return try await dotNsResolver.stageContent(dotNsName: contentId)
        } catch DotNsContractError.contentHashNotFound {
            resolverFactory.logger.warning("Product \(productId) has no published content at \(contentId)")
            return nil
        }
    }
}
