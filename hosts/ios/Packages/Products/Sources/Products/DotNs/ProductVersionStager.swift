import Foundation
import SDKLogger

public struct StagedProductVersion: Equatable, Sendable {
    public let version: ProductVersion
    public let resolved: ResolvedProduct
    public let appContent: DotNsResolvedContent
}

public protocol ProductVersionStaging: Sendable {
    func stage(_ productId: ProductId) async throws -> StagedProductVersion
}

public actor ProductVersionStager {
    public struct Storage: Sendable {
        let content: DotNsContentStorageProtocol
        let contentHashes: ContentHashCacheProtocol

        public init(content: DotNsContentStorageProtocol, contentHashes: ContentHashCacheProtocol) {
            self.content = content
            self.contentHashes = contentHashes
        }
    }

    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let storage: Storage
    private let hostProvider: ProductHostProviding
    private let logger: SDKLoggerProtocol

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: Storage,
        hostProvider: ProductHostProviding,
        logger: SDKLoggerProtocol
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.storage = storage
        self.hostProvider = hostProvider
        self.logger = logger
    }
}

extension ProductVersionStager: ProductVersionStaging {
    public func stage(_ productId: ProductId) async throws -> StagedProductVersion {
        let log = DotNsReadLog()
        let dotNsResolver = DotNsResolver(
            contractApi: RecordingDotNsContractApi(contractApi: contractApi, log: log),
            carFetcher: carFetcher,
            contentStorage: storage.content,
            contentHashCache: storage.contentHashes
        )

        let resolved = try await ProductResolver(
            dotNsResolver: dotNsResolver,
            hostProvider: hostProvider,
            logger: logger
        ).resolve(productId)

        let appContent = try await dotNsResolver.stageContent(dotNsName: resolved.appContentId)

        if let worker = resolved.executables.worker {
            _ = try await dotNsResolver.stageContent(dotNsName: worker.identifier)
        }

        return StagedProductVersion(version: log.version(of: resolved.id), resolved: resolved, appContent: appContent)
    }
}
