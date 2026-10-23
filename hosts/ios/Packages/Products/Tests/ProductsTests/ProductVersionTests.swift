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
        let staged = try await makeStager().stage(baseName)
        try makeApplier().apply(staged.version)
        let calls = contracts.callCount

        let cached = try makeCachedStack()
        let resolved = try await cached.products.resolve(baseName)
        let appContent = try await cached.dotNs.resolveContent(dotNsName: resolved.appContentId)

        #expect(resolved == staged.resolved)
        #expect(appContent == staged.appContent)
        #expect(contracts.callCount == calls)
        #expect(fetcher.fetchCount == 2)
    }

    @Test func legacyProductOpensFromItsAppliedVersion() async throws {
        contracts.set(contentHash: Self.appHash, name: baseName)
        let staged = try await makeStager().stage(baseName)
        try makeApplier().apply(staged.version)

        let resolved = try await makeCachedStack().products.resolve(baseName)

        #expect(!resolved.hasManifest)
        #expect(resolved == staged.resolved)
    }

    @Test func stagedVersionIsNotServedUntilApplied() async throws {
        publishManifestProduct()
        let applied = try await makeStager().stage(baseName)
        try makeApplier().apply(applied.version)

        contracts.set(contentHash: Self.updatedAppHash, name: appName)
        let update = try await makeStager().stage(baseName)

        #expect(try await cachedAppContent() == applied.appContent)

        try makeApplier().apply(update.version)

        #expect(try await cachedAppContent() == update.appContent)
    }

    @Test func runningWorkerKeepsReadingTheAppliedVersionUntilTheStagedOneIsApplied() async throws {
        publishManifestProduct()
        try await makeApplier().apply(makeStager().stage(baseName).version)
        let workerFiles = CompositeProductFileProvider(
            dotNsContentStorage: storage,
            chatScriptStorage: FileChatScriptStorage(),
            contentHashCache: contentHashCache
        )

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName)
        let update = try await makeStager().stage(baseName)

        #expect(workerFiles.load(for: workerName, relativePath: "worker.js") == Data(Self.workerHash.toHex().utf8))

        try makeApplier().apply(update.version)

        #expect(workerFiles
            .load(for: workerName, relativePath: "worker.js") == Data(Self.updatedWorkerHash.toHex().utf8))
    }

    @Test func versionChangesWhenOnlyTheWorkerIsRepublished() async throws {
        publishManifestProduct()
        let first = try await makeStager().stage(baseName).version
        let unchanged = try await makeStager().stage(baseName).version

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName)
        let republished = try await makeStager().stage(baseName).version

        #expect(unchanged == first)
        #expect(republished != first)
    }

    @Test func failedStagingKeepsServingTheAppliedVersion() async throws {
        publishManifestProduct()
        let applied = try await makeStager().stage(baseName)
        try makeApplier().apply(applied.version)

        contracts.failAllReads(with: Unreachable())

        await #expect(throws: Unreachable.self) {
            try await self.makeStager().stage(self.baseName)
        }

        #expect(try await cachedAppContent() == applied.appContent)
    }

    @Test func productNeverAppliedIsNotServedFromCache() async throws {
        publishManifestProduct()
        _ = try await makeStager().stage(baseName)

        await #expect(throws: DotNsSnapshotError.self) {
            try await self.makeCachedStack().products.resolve(self.baseName)
        }
    }

    @Test func archiveMissingFromDiskIsNotDownloadedWhenOpeningFromCache() async throws {
        publishManifestProduct()
        let staged = try await makeStager().stage(baseName)
        try makeApplier().apply(staged.version)

        try storage.deleteAll()

        await #expect(throws: DotNsSnapshotError.archiveNotOnDisk) {
            try await self.makeCachedStack().dotNs.resolveContent(dotNsName: staged.resolved.appContentId)
        }

        #expect(fetcher.fetchCount == 2)
    }
}

private extension ProductVersionTests {
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

    func makeApplier() throws -> ProductVersionApplier {
        try ProductVersionApplier(snapshot: makeSnapshot(), contentHashCache: contentHashCache)
    }

    func makeStager() -> ProductVersionStager {
        ProductVersionStager(
            contractApi: contracts,
            carFetcher: fetcher,
            storage: ProductVersionStager.Storage(content: storage, contentHashes: contentHashCache),
            hostProvider: ProductHostFactory(tldProvider: StubTldProvider()),
            logger: SilentLogger()
        )
    }

    func makeCachedStack() throws -> Stack {
        let dotNs = try DotNsResolver(
            contractApi: SnapshotDotNsContractApi(snapshot: makeSnapshot(), contentHashCache: contentHashCache),
            carFetcher: SnapshotCarFetcher(),
            contentStorage: storage,
            contentHashCache: contentHashCache
        )
        let tldProvider = SnapshotDotNsTldProvider(tldProvider: StubTldProvider(resolveError: Unreachable()))
        let products = ProductResolver(
            dotNsResolver: dotNs,
            hostProvider: ProductHostFactory(tldProvider: tldProvider),
            logger: SilentLogger()
        )

        return Stack(products: products, dotNs: dotNs)
    }

    func cachedAppContent() async throws -> DotNsResolvedContent {
        let cached = try makeCachedStack()
        let resolved = try await cached.products.resolve(baseName)
        return try await cached.dotNs.resolveContent(dotNsName: resolved.appContentId)
    }
}
