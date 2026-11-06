import Foundation
import CarParser
import IssueMonitoring

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
    /// Never re-reads the chain once an archive is applied; newer ones arrive through `ProductContentLoader`.
    func resolveToLocalURL(dotNsName: String) async throws -> URL

    /// Clear all cached content and hash mappings.
    func clearCache() throws
}

public final class DotNsResolver: DotNsResolverProtocol {
    private let contractApi: DotNsContractApiProtocol
    private let carFetcher: CarFetcherProtocol
    private let contentStorage: DotNsContentStorageProtocol
    private let contentHashCache: ContentHashCacheProtocol
    private let telemetry: HostLoadTelemetry

    public init(
        contractApi: DotNsContractApiProtocol,
        carFetcher: CarFetcherProtocol,
        contentStorage: DotNsContentStorageProtocol,
        contentHashCache: ContentHashCacheProtocol,
        telemetry: HostLoadTelemetry = .disabled
    ) {
        self.contractApi = contractApi
        self.carFetcher = carFetcher
        self.contentStorage = contentStorage
        self.contentHashCache = contentHashCache
        self.telemetry = telemetry
    }

    public func resolveToLocalURL(dotNsName: String) async throws -> URL {
        let startedAt = telemetry.loadStepStart()

        if let applied = appliedContentDirectory(of: dotNsName) {
            report(BulletinFetch.applied(domain: dotNsName, startedAt: startedAt), outcome: .cacheHit, bytes: nil)
            return applied
        }

        let content = try await stageContent(dotNsName: dotNsName)

        if appliedContentDirectory(of: dotNsName) == nil {
            contentHashCache.putContentHash(name: dotNsName, hash: content.contentHash)
        }

        return content.localURL
    }

    public func stageContent(
        dotNsName: String,
        onPhase: @escaping ProductLoadPhaseHandler = { _ in }
    ) async throws -> DotNsResolvedContent {
        let lookupStartedAt = telemetry.loadStepStart()
        let contentHashBytes: Data

        do {
            contentHashBytes = try await contractApi.resolveContentHash(dotNsName: dotNsName)
        } catch {
            if !(error is CancellationError) {
                let failed = BulletinFetch.failedLookup(domain: dotNsName, lookupStartedAt: lookupStartedAt)
                report(failed, outcome: .error, bytes: nil)
            }
            throw error
        }

        let hexHash = contentHashBytes.toHex()
        let fetch = BulletinFetch.afterLookup(
            domain: dotNsName,
            lookupStartedAt: lookupStartedAt,
            startedAt: lookupStartedAt == nil ? nil : telemetry.now()
        )

        if let cachedDir = contentStorage.getContentDirectory(contentHash: hexHash) {
            // The content hash was a real chain read; only the download was saved.
            report(fetch, outcome: .ok, bytes: nil)
            return DotNsResolvedContent(contentHash: hexHash, localURL: cachedDir)
        }

        var carBytes: Int64?

        do {
            let carFileURL = try await carFetcher.fetchCarToFile(
                contentHash: contentHashBytes,
                onProgress: { [isRecording = fetch != nil] downloaded, total in
                    if isRecording {
                        carBytes = downloaded
                    }
                    onPhase(.downloading(downloadedBytes: downloaded, totalBytes: total))
                }
            )

            onPhase(.unpacking)

            let contentDir = try saveResolvedContent(
                carFileURL: carFileURL,
                hexHash: hexHash,
                dotNsName: dotNsName
            )

            report(fetch, outcome: .ok, bytes: carBytes)
            return DotNsResolvedContent(contentHash: hexHash, localURL: contentDir)
        } catch {
            if !(error is CancellationError) {
                report(fetch, outcome: .error, bytes: carBytes)
            }
            throw error
        }
    }

    public func clearCache() throws {
        try contentStorage.deleteAll()
        contentHashCache.clearAll()
        contractApi.clearCache()
    }
}

private struct BulletinFetch {
    enum ContentHashRead {
        case took(Int)
        case failedAtReport
        case notRead
    }

    let domain: String
    let startedAt: Int
    let contentHashRead: ContentHashRead

    static func afterLookup(domain: String, lookupStartedAt: Int?, startedAt: Int?) -> BulletinFetch? {
        guard let lookupStartedAt, let startedAt else { return nil }

        return BulletinFetch(domain: domain, startedAt: startedAt, contentHashRead: .took(startedAt - lookupStartedAt))
    }

    static func failedLookup(domain: String, lookupStartedAt: Int?) -> BulletinFetch? {
        lookupStartedAt.map { BulletinFetch(domain: domain, startedAt: $0, contentHashRead: .failedAtReport) }
    }

    static func applied(domain: String, startedAt: Int?) -> BulletinFetch? {
        startedAt.map { BulletinFetch(domain: domain, startedAt: $0, contentHashRead: .notRead) }
    }

    func contentHashMilliseconds(at now: Int) -> Int? {
        switch contentHashRead {
        case let .took(milliseconds): milliseconds
        case .failedAtReport: now - startedAt
        case .notRead: nil
        }
    }
}

private extension DotNsResolver {
    static let fetchStep = "host.bulletin.fetch"

    func report(_ fetch: BulletinFetch?, outcome: HostEventOutcome, bytes: Int64?) {
        guard
            let fetch,
            let context = telemetry.context,
            telemetry.shouldReport(Self.fetchStep, productID: fetch.domain, outcome: outcome)
        else { return }

        telemetry.record { origin, now in
            .bulletinFetch(
                domain: fetch.domain,
                outcome: outcome,
                trigger: context.trigger,
                durationMilliseconds: now - fetch.startedAt,
                contentHashMilliseconds: fetch.contentHashMilliseconds(at: now),
                bytes: bytes.map { Int($0) },
                hostSessionID: context.sessionID,
                origin: origin,
                monotonicMilliseconds: now
            )
        }
    }
}

extension DotNsResolver {
    private func appliedContentDirectory(of dotNsName: String) -> URL? {
        contentHashCache.getContentHash(name: dotNsName).flatMap(contentStorage.getContentDirectory(contentHash:))
    }

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
