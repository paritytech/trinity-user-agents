import Foundation
import SDKLogger

public struct FetchedProductVersion: Equatable, Sendable {
    public let version: ProductVersion
    public let resolved: ResolvedProduct
    public let appContent: DotNsResolvedContent

    public init(version: ProductVersion, resolved: ResolvedProduct, appContent: DotNsResolvedContent) {
        self.version = version
        self.resolved = resolved
        self.appContent = appContent
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
    private let logger: SDKLoggerProtocol

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        storage: DotNsLocalStorage,
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

        let dotNsResolver = makeDotNsResolver(carFetcher: carFetcher)

        let manifest = try await ProductResolver(
            metadataRepository: ProductMetadataRepository(contractApi: contractApi),
            hostProvider: hostProvider,
            logger: logger
        ).resolveProductManifest(productId)
        let resolved = manifest.product

        let appContent = try await makeDotNsResolver(
            carFetcher: ProgressReportingCarFetcher(carFetcher: carFetcher, onPhase: onPhase)
        ).stageContent(dotNsName: resolved.appContentId)
        var contentHashes = [resolved.appContentId: appContent.contentHash]

        if let worker = resolved.executables.worker,
           let workerContent = try await stageWorker(worker.identifier, of: resolved.id, using: dotNsResolver) {
            contentHashes[worker.identifier] = workerContent.contentHash
        }

        return FetchedProductVersion(
            version: ProductVersion(productId: resolved.id, records: manifest.records, contentHashes: contentHashes),
            resolved: resolved,
            appContent: appContent
        )
    }
}

private extension ProductUpdateFetcher {
    func makeDotNsResolver(carFetcher: CarFetcherProtocol) -> DotNsResolver {
        DotNsResolver(
            contractApi: contractApi,
            carFetcher: carFetcher,
            contentStorage: storage.content,
            contentHashCache: storage.contentHashes
        )
    }

    func stageWorker(
        _ workerId: ProductId,
        of productId: ProductId,
        using dotNsResolver: DotNsResolver
    ) async throws -> DotNsResolvedContent? {
        do {
            return try await dotNsResolver.stageContent(dotNsName: workerId)
        } catch DotNsContractError.contentHashNotFound {
            logger.warning("Product \(productId) declares a worker with no published content")
            return nil
        }
    }
}

private struct ProgressReportingCarFetcher: CarFetcherProtocol {
    let carFetcher: CarFetcherProtocol
    let onPhase: ProductLoadPhaseHandler

    func fetchCarToFile(
        contentHash: Data,
        onProgress: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void
    ) async throws -> URL {
        let fileURL = try await carFetcher.fetchCarToFile(contentHash: contentHash) { [onPhase] downloaded, total in
            onProgress(downloaded, total)
            onPhase(.downloading(downloadedBytes: downloaded, totalBytes: total))
        }

        onPhase(.unpacking)
        return fileURL
    }
}
