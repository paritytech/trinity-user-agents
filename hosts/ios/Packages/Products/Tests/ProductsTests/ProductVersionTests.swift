import Foundation
import SubstrateSdk
import Testing
@testable import Products

final class ProductVersionTests {
    private struct Unreachable: Error {}

    private static let appHash = Data(repeating: 0xA1, count: 34)
    private static let updatedAppHash = Data(repeating: 0xA2, count: 34)
    private static let workerHash = Data(repeating: 0xB1, count: 34)
    private static let updatedWorkerHash = Data(repeating: 0xB2, count: 34)

    private let suiteName = "ProductVersionTests.\(UUID().uuidString)"
    private let baseName = "shop\(UUID().uuidString.prefix(8).lowercased()).dot"
    private let contracts = StubDotNsContractApi()
    private let fetcher = StubCarFetcher()
    private let storage = InMemoryDotNsContentStorage()
    private let contentHashCache = ContentHashCache()

    deinit {
        UserDefaults().removePersistentDomain(forName: suiteName)
        [baseName, appName, workerName].forEach(contentHashCache.removeContentHash(name:))
    }

    @Test func appliedVersionOpensWithoutTheChainOrTheGateway() async throws {
        publishManifestProduct()
        let staged = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(staged.version)
        let calls = contracts.callCount

        let cached = try await makeCache().open(baseName)

        #expect(cached.resolved == staged.resolved)
        #expect(cached.appContent == staged.appContent)
        #expect(cached.version == staged.version)
        #expect(contracts.callCount == calls)
        #expect(fetcher.fetchCount == 2)
    }

    @Test func legacyProductOpensFromItsAppliedVersion() async throws {
        contracts.set(contentHash: Self.appHash, name: baseName)
        let staged = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(staged.version)

        let resolved = try await makeCache().open(baseName).resolved

        #expect(!resolved.hasManifest)
        #expect(resolved == staged.resolved)
    }

    @Test func stagedVersionIsNotServedUntilApplied() async throws {
        publishManifestProduct()
        let applied = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(applied.version)

        contracts.set(contentHash: Self.updatedAppHash, name: appName)
        let update = try await makeFetcher().fetch(baseName)

        #expect(try await makeCache().open(baseName).appContent == applied.appContent)

        try await makeCache().apply(update.version)

        #expect(try await makeCache().open(baseName).appContent == update.appContent)
    }

    @Test func runningWorkerKeepsReadingTheAppliedVersionUntilTheStagedOneIsApplied() async throws {
        publishManifestProduct()
        try await makeCache().apply(makeFetcher().fetch(baseName).version)
        let workerFiles = CompositeProductFileProvider(
            dotNsContentStorage: storage,
            chatScriptStorage: FileChatScriptStorage(),
            contentHashCache: contentHashCache
        )

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName)
        let update = try await makeFetcher().fetch(baseName)

        #expect(workerFiles.load(for: workerName, relativePath: "worker.js") == Data(Self.workerHash.toHex().utf8))

        try await makeCache().apply(update.version)

        #expect(workerFiles
            .load(for: workerName, relativePath: "worker.js") == Data(Self.updatedWorkerHash.toHex().utf8))
    }

    @Test func versionChangesWhenOnlyTheWorkerIsRepublished() async throws {
        publishManifestProduct()
        let first = try await makeFetcher().fetch(baseName).version
        let unchanged = try await makeFetcher().fetch(baseName).version

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName)
        let republished = try await makeFetcher().fetch(baseName).version

        #expect(unchanged == first)
        #expect(republished != first)
    }

    @Test func failedStagingKeepsServingTheAppliedVersion() async throws {
        publishManifestProduct()
        let applied = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(applied.version)

        contracts.failAllReads(with: Unreachable())

        await #expect(throws: Unreachable.self) {
            try await self.makeFetcher().fetch(self.baseName)
        }

        #expect(try await makeCache().open(baseName).appContent == applied.appContent)
    }

    @Test func productNeverAppliedIsNotServedFromCache() async throws {
        publishManifestProduct()
        _ = try await makeFetcher().fetch(baseName)

        await #expect(throws: DotNsSnapshotError.self) {
            try await self.makeCache().open(self.baseName)
        }
    }

    @Test func archiveMissingFromDiskIsNotDownloadedWhenOpeningFromCache() async throws {
        publishManifestProduct()
        let staged = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(staged.version)

        try storage.deleteAll()

        await #expect(throws: DotNsSnapshotError.archiveNotOnDisk) {
            try await self.makeCache().open(self.baseName)
        }

        #expect(fetcher.fetchCount == 2)
    }

    @Test func cachedVersionMissingTheWorkerArchiveDiffersSoRefreshRestoresIt() async throws {
        publishManifestProduct()
        let fetched = try await makeFetcher().fetch(baseName)
        try await makeCache().apply(fetched.version)

        try storage.deleteContent(contentHash: Self.workerHash.toHex())

        #expect(try await makeCache().open(baseName).version != fetched.version)
    }

    @Test func cachedOpenNeedsAKnownTldInsteadOfFallingBackToLegacy() async throws {
        publishManifestProduct()
        try await makeCache().apply(makeFetcher().fetch(baseName).version)

        await #expect(throws: DotNsContractError.self) {
            try await self.makeCache(tldProvider: FailingTldProvider()).open(self.baseName)
        }
    }

    @Test func fetchNeedsAReadableTldInsteadOfFallingBackToLegacy() async throws {
        publishManifestProduct()

        await #expect(throws: ProductUpdateFetchError.tldUnavailable) {
            try await self.makeFetcher(tldProvider: FailingTldProvider()).fetch(self.baseName)
        }

        #expect(contracts.callCount == 0)
    }
}

private extension ProductVersionTests {
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
            text: #"""
            {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"worker.js",
             "includes":{"chat":true,"pocket":false}}
            """#,
            name: workerName,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(contentHash: Self.appHash, name: appName)
        contracts.set(contentHash: Self.workerHash, name: workerName)
    }

    func makeSnapshot() throws -> DotNsRecordSnapshot {
        try DotNsRecordSnapshot(defaults: #require(UserDefaults(suiteName: suiteName)))
    }

    func makeCache(tldProvider: DotNsTldProviding = StubTldProvider(resolveError: Unreachable())) throws
        -> ProductVersionCache {
        try ProductVersionCache(
            snapshot: makeSnapshot(),
            storage: DotNsLocalStorage(content: storage, contentHashes: contentHashCache),
            tldProvider: tldProvider,
            logger: SilentLogger()
        )
    }

    func makeFetcher(tldProvider: DotNsTldProviding = StubTldProvider()) -> ProductUpdateFetcher {
        ProductUpdateFetcher(
            contractApi: contracts,
            carFetcher: fetcher,
            storage: DotNsLocalStorage(content: storage, contentHashes: contentHashCache),
            hostProvider: ProductHostFactory(tldProvider: tldProvider),
            logger: SilentLogger()
        )
    }
}
