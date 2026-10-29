import Foundation
import SDKLogger

public enum ProductUpdateFetchError: Error, Equatable {
    case tldUnavailable
}

public protocol ProductUpdateFetching: Sendable {
    func fetchManifest(_ productId: ProductId) async throws -> ProductManifest
    func stage(_ contentId: ProductId, onPhase: @escaping ProductLoadPhaseHandler) async throws -> DotNsResolvedContent
}

public extension ProductUpdateFetching {
    func stage(_ contentId: ProductId) async throws -> DotNsResolvedContent {
        try await stage(contentId, onPhase: { _ in })
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
    public func fetchManifest(_ productId: ProductId) async throws -> ProductManifest {
        do {
            _ = try await hostProvider.resolveHost(rawString: productId.lowercased())
        } catch {
            throw ProductUpdateFetchError.tldUnavailable
        }

        return try await resolverFactory.makeResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider
        ).resolveProductManifest(productId)
    }

    public func stage(
        _ contentId: ProductId,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> DotNsResolvedContent {
        try await DotNsResolver(
            contractApi: contractApi,
            carFetcher: carFetcher,
            contentStorage: storage.content,
            contentHashCache: storage.contentHashes
        ).stageContent(dotNsName: contentId, onPhase: onPhase)
    }
}
