import Foundation
import CarParser

public enum DotNsResolverError: Error {
    case resolutionFailed(String)
    case contentHashNotFound(String)
    case carParseFailed(Error)
    case storageFailed(Error)
}

public struct DotNsResolvedContent: Equatable, Sendable {
    public let contentHash: String
    public let localURL: URL

    public init(contentHash: String, localURL: URL) {
        self.contentHash = contentHash
        self.localURL = localURL
    }
}

/// Protocol for resolving .dot domain names to local content directories.
public protocol DotNsResolverProtocol {
    /// Resolve a .dot domain name to a local URL containing the extracted content files.
    func resolveToLocalURL(dotNsName: String) async throws -> URL

    /// Clear all cached content and hash mappings.
    func clearCache() throws
}

public final class DotNsResolver: DotNsResolverProtocol {
    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let contentStorage: DotNsContentStorageProtocol
    private let contentHashCache: ContentHashCacheProtocol

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        contentStorage: DotNsContentStorageProtocol,
        contentHashCache: ContentHashCacheProtocol
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.contentStorage = contentStorage
        self.contentHashCache = contentHashCache
    }

    public func resolveToLocalURL(dotNsName: String) async throws -> URL {
        let content = try await stageContent(dotNsName: dotNsName)
        contentHashCache.putContentHash(name: dotNsName, hash: content.contentHash)
        return content.localURL
    }

    public func stageContent(
        dotNsName: String,
        onPhase: @escaping ProductLoadPhaseHandler = { _ in }
    ) async throws -> DotNsResolvedContent {
        let contentHashBytes = try await contractApi.resolveContentHash(dotNsName: dotNsName)
        let hexHash = contentHashBytes.toHex()

        if let cachedDir = contentStorage.getContentDirectory(contentHash: hexHash) {
            return DotNsResolvedContent(contentHash: hexHash, localURL: cachedDir)
        }

        let carFileURL = try await carFetcher.fetchCarToFile(
            contentHash: contentHashBytes,
            onProgress: { downloaded, total in
                onPhase(.downloading(downloadedBytes: downloaded, totalBytes: total))
            }
        )

        onPhase(.unpacking)

        let contentDir = try saveResolvedContent(
            carFileURL: carFileURL,
            hexHash: hexHash,
            dotNsName: dotNsName
        )

        return DotNsResolvedContent(contentHash: hexHash, localURL: contentDir)
    }

    public func clearCache() throws {
        try contentStorage.deleteAll()
        contentHashCache.clearAll()
        contractApi.clearCache()
    }
}

extension DotNsResolver {
    private func saveResolvedContent(carFileURL: URL, hexHash: String, dotNsName: String) throws -> URL {
        // Removing temp item
        defer { try? FileManager.default.removeItem(at: carFileURL) }

        do {
            try contentStorage.saveContent(contentHash: hexHash) { writer in
                try CarParser.unpack(fileURL: carFileURL, writer: writer)
            }
        } catch let error as CarParserError {
            throw DotNsResolverError.carParseFailed(error)
        } catch {
            throw DotNsResolverError.storageFailed(error)
        }

        guard let contentDir = contentStorage.getContentDirectory(contentHash: hexHash) else {
            throw DotNsResolverError.resolutionFailed("Failed to retrieve saved content for \(dotNsName)")
        }

        return contentDir
    }
}
