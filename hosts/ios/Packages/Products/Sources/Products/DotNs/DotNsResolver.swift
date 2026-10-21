import Foundation
import CarParser
import AsyncExtensions

public enum DotNsResolverError: Error {
    case resolutionFailed(String)
    case contentHashNotFound(String)
    case carParseFailed(Error)
    case storageFailed(Error)
}

/// A build: the content one content hash points at, unpacked locally.
public struct DotNsResolvedContent: Sendable, Equatable {
    /// Hex content hash identifying the build.
    public let contentHash: String
    /// Directory holding the unpacked build files.
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

    /// Read the current hex content hash of a .dot domain from chain. Never downloads content.
    func resolveContentHash(dotNsName: String) async throws -> String

    /// Resolve a .dot domain name to its current build, downloading and unpacking it when not cached.
    func resolveContent(dotNsName: String) async throws -> DotNsResolvedContent

    /// Fetch a metadata entry for a .dot domain (e.g., "url", "description").
    func getMetadataEntry(dotNsName: String, key: String) async throws -> String?

    /// Observe load progress for a `.dot` domain. Emits `.idle` when nothing is in flight.
    func progressStream(dotNsName: String) -> AnyAsyncSequence<DotNsLoadProgress>

    /// Clear all cached content and hash mappings.
    func clearCache() throws
}

public final class DotNsResolver: DotNsResolverProtocol {
    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let contentStorage: DotNsContentStorageProtocol
    private let contentHashCache: ContentHashCacheProtocol
    private let progressRegistry: any DotNsLoadProgressRegistryProtocol

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
        progressRegistry = DotNsLoadProgressRegistry()
    }

    // TODO: Current implementation always fetch content hash from contract.
    // Target solution should warn a user if hash changed monitoring the cache
    public func resolveToLocalURL(dotNsName: String) async throws -> URL {
        try await resolveContent(dotNsName: dotNsName).localURL
    }

    public func resolveContentHash(dotNsName: String) async throws -> String {
        try await contractApi.resolveContentHash(dotNsName: dotNsName).toHex()
    }

    public func resolveContent(dotNsName: String) async throws -> DotNsResolvedContent {
        let contentHashBytes = try await contractApi.resolveContentHash(dotNsName: dotNsName)
        let hexHash = contentHashBytes.toHex()

        if let cachedDir = contentStorage.getContentDirectory(contentHash: hexHash) {
            contentHashCache.putContentHash(name: dotNsName, hash: hexHash)
            progressRegistry.markCompleted(domain: dotNsName)
            return DotNsResolvedContent(contentHash: hexHash, localURL: cachedDir)
        }

        do {
            progressRegistry.markResolved(domain: dotNsName)

            let carFileURL = try await carFetcher.fetchCarToFile(
                contentHash: contentHashBytes,
                onProgress: { [progressRegistry] downloaded, total in
                    progressRegistry.markDownload(domain: dotNsName, downloaded: downloaded, total: total)
                }
            )

            let contentDir = try saveResolvedContent(
                carFileURL: carFileURL,
                hexHash: hexHash,
                dotNsName: dotNsName
            )

            progressRegistry.markCompleted(domain: dotNsName)
            return DotNsResolvedContent(contentHash: hexHash, localURL: contentDir)
        } catch {
            progressRegistry.markFailed(domain: dotNsName, error: error)
            throw error
        }
    }

    public func progressStream(dotNsName: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        progressRegistry.observe(domain: dotNsName)
    }

    public func getMetadataEntry(dotNsName: String, key: String) async throws -> String? {
        try await contractApi.getMetadata(dotNsName: dotNsName, key: key)
    }

    public func clearCache() throws {
        try contentStorage.deleteAll()
        contentHashCache.clearAll()
        progressRegistry.clear()
        contractApi.clearCache()
    }
}

extension DotNsResolver {
    private func saveResolvedContent(carFileURL: URL, hexHash: String, dotNsName: String) throws -> URL {
        progressRegistry.markUnpacking(domain: dotNsName)
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

        contentHashCache.putContentHash(name: dotNsName, hash: hexHash)

        guard let contentDir = contentStorage.getContentDirectory(contentHash: hexHash) else {
            throw DotNsResolverError.resolutionFailed("Failed to retrieve saved content for \(dotNsName)")
        }

        return contentDir
    }
}
