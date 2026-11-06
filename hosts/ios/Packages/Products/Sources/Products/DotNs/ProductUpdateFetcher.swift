import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import SDKLogger

public enum ProductUpdateFetchError: Error, Equatable {
    case tldUnavailable
}

public protocol ProductUpdateFetching: Sendable {
    func fetchManifest(_ productId: ProductId) async throws -> ProductManifest
    func stage(
        _ contentId: ProductId,
        onPhase: @escaping ProductLoadPhaseHandler
    ) async throws -> DotNsResolvedContent
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
    private let telemetry: HostLoadTelemetry

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: DotNsLocalStorage,
        hostProvider: ProductHostProviding,
        resolverFactory: ProductResolverFactory,
        telemetry: HostLoadTelemetry = .disabled
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.storage = storage
        self.hostProvider = hostProvider
        self.resolverFactory = resolverFactory
        self.telemetry = telemetry
    }
}

extension ProductUpdateFetcher: ProductUpdateFetching {
    public func fetchManifest(_ productId: ProductId) async throws -> ProductManifest {
        do {
            _ = try await hostProvider.resolveHost(rawString: productId.rawValue)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw ProductUpdateFetchError.tldUnavailable
        }

        return try await resolverFactory.makeResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider,
            telemetry: telemetry
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
            contentHashCache: storage.contentHashes,
            telemetry: telemetry
        ).stageContent(dotNsName: contentId.rawValue, onPhase: onPhase)
    }
}
