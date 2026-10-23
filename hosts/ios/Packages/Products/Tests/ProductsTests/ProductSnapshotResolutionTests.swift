import Foundation
import Testing
@testable import Products

final class ProductSnapshotResolutionTests {
    private struct Unreachable: Error {}

    private static let contentHash = Data(repeating: 0xCD, count: 34)

    private let suiteName = "ProductSnapshotResolutionTests.\(UUID().uuidString)"
    private let baseName = "shop\(UUID().uuidString.prefix(8).lowercased()).dot"
    private let contracts = StubDotNsContractApi()
    private let fetcher = StubCarFetcher()
    private let storage = InMemoryDotNsContentStorage()
    private let contentHashCache = ContentHashCache()

    deinit {
        UserDefaults().removePersistentDomain(forName: suiteName)
        [baseName, appName, workerName].forEach(contentHashCache.removeContentHash(name:))
    }

    @Test func manifestProductOpensFromTheSnapshotWithoutTheChain() async throws {
        publishManifestProduct()
        let fresh = try freshStack()

        let resolved = try await fresh.products.resolve(baseName)
        let contentURL = try await fresh.dotNs.resolveToLocalURL(dotNsName: resolved.appContentId)
        let freshCalls = contracts.callCount

        let cached = try snapshotStack()
        let cachedResolved = try await cached.products.resolve(baseName)
        let cachedURL = try await cached.dotNs.resolveToLocalURL(dotNsName: cachedResolved.appContentId)

        #expect(resolved.hasManifest)
        #expect(cachedResolved == resolved)
        #expect(cachedURL == contentURL)
        #expect(contracts.callCount == freshCalls)
        #expect(fetcher.fetchCount == 1)
    }

    @Test func legacyProductOpensFromTheSnapshot() async throws {
        contracts.set(contentHash: Self.contentHash, name: baseName)
        let fresh = try freshStack()

        let resolved = try await fresh.products.resolve(baseName)
        _ = try await fresh.dotNs.resolveToLocalURL(dotNsName: resolved.appContentId)

        let cachedResolved = try await snapshotStack().products.resolve(baseName)

        #expect(!cachedResolved.hasManifest)
        #expect(cachedResolved == resolved)
    }

    @Test func productNeverResolvedIsNotServedFromTheSnapshot() async throws {
        publishManifestProduct()

        await #expect(throws: DotNsSnapshotError.self) {
            try await self.snapshotStack().products.resolve(self.baseName)
        }

        #expect(contracts.callCount == 0)
    }

    @Test func archiveMissingFromDiskIsNotDownloadedForTheSnapshot() async throws {
        publishManifestProduct()
        let fresh = try freshStack()
        let resolved = try await fresh.products.resolve(baseName)
        _ = try await fresh.dotNs.resolveToLocalURL(dotNsName: resolved.appContentId)

        try storage.deleteAll()

        await #expect(throws: DotNsSnapshotError.archiveNotOnDisk) {
            try await self.snapshotStack().dotNs.resolveToLocalURL(dotNsName: resolved.appContentId)
        }

        #expect(fetcher.fetchCount == 1)
    }

    @Test func failedReadsKeepWhatTheSnapshotRecorded() async throws {
        publishManifestProduct()
        let resolved = try await freshStack().products.resolve(baseName)

        contracts.failAllReads(with: Unreachable())

        await #expect(throws: Unreachable.self) {
            try await self.freshStack().products.resolve(self.baseName)
        }

        #expect(try await snapshotStack().products.resolve(baseName) == resolved)
    }
}

private extension ProductSnapshotResolutionTests {
    struct Stack {
        let products: ProductResolver
        let dotNs: DotNsResolver
    }

    var appName: String { ProductManifestRecords.subname(base: baseName, kind: .app) }
    var workerName: String { ProductManifestRecords.subname(base: baseName, kind: .worker) }

    func publishManifestProduct() {
        contracts.set(
            text: #"{"$v":1,"displayName":"Shop","description":"","icon":{"cid":"bafyicon","format":"png"}}"#,
            name: baseName,
            key: ProductManifestRecords.rootKey
        )
        contracts.set(
            text: #"{"$v":1,"kind":"app","appVersion":[1,0,0]}"#,
            name: appName,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(
            text: #"{"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"worker.js","includes":{"chat":true}}"#,
            name: workerName,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(contentHash: Self.contentHash, name: appName)
    }

    func makeSnapshot() throws -> DotNsRecordSnapshot {
        try DotNsRecordSnapshot(defaults: #require(UserDefaults(suiteName: suiteName)))
    }

    func freshStack() throws -> Stack {
        let dotNs = try DotNsResolver(
            contractApi: RecordingDotNsContractApi(contractApi: contracts, snapshot: makeSnapshot()),
            carFetcher: fetcher,
            contentStorage: storage,
            contentHashCache: contentHashCache
        )

        return Stack(products: makeProductResolver(dotNs: dotNs, tldProvider: StubTldProvider()), dotNs: dotNs)
    }

    func snapshotStack() throws -> Stack {
        let dotNs = try DotNsResolver(
            contractApi: SnapshotDotNsContractApi(snapshot: makeSnapshot(), contentHashCache: contentHashCache),
            carFetcher: SnapshotCarFetcher(),
            contentStorage: storage,
            contentHashCache: contentHashCache
        )
        let tldProvider = SnapshotDotNsTldProvider(tldProvider: StubTldProvider(resolveError: Unreachable()))

        return Stack(products: makeProductResolver(dotNs: dotNs, tldProvider: tldProvider), dotNs: dotNs)
    }

    func makeProductResolver(dotNs: DotNsResolver, tldProvider: DotNsTldProviding) -> ProductResolver {
        ProductResolver(
            dotNsResolver: dotNs,
            hostProvider: ProductHostFactory(tldProvider: tldProvider),
            logger: SilentLogger()
        )
    }
}
