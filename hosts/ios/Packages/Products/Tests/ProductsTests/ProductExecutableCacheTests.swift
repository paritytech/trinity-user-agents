import TrUAPIHost
import Foundation
import SubstrateSdk
import Testing
@testable import Products

final class ProductExecutableCacheTests {
    private struct Unreachable: Error {}

    private static let appHash = Data(repeating: 0xA1, count: 34)
    private static let updatedAppHash = Data(repeating: 0xA2, count: 34)
    private static let workerHash = Data(repeating: 0xB1, count: 34)
    private static let widgetHash = Data(repeating: 0xC1, count: 34)
    private static let updatedWorkerHash = Data(repeating: 0xB2, count: 34)

    private let manifestsDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("ProductExecutableCacheTests-\(UUID().uuidString)")
    private let baseName = try! ProductId("shop\(UUID().uuidString.prefix(8).lowercased()).dot")
    private let contracts = StubDotNsContractApi()
    private let fetcher = StubCarFetcher()
    private let storage = InMemoryDotNsContentStorage()
    private let contentHashCache = ContentHashCache()

    deinit {
        try? FileManager.default.removeItem(at: manifestsDirectory)
        [baseName, appName, widgetName, workerName].forEach { contentHashCache.removeContentHash(name: $0.rawValue) }
    }

    @Test func savedManifestResolvesWithoutTheChain() async throws {
        publishManifestProduct()
        let fetched = try await makeFetcher().fetchManifest(baseName)
        await makeCache().adopt(fetched)
        let calls = contracts.callCount

        let cached = try await makeCache().manifest(baseName)

        #expect(cached == fetched)
        #expect(contracts.callCount == calls)
    }

    @Test func legacyProductResolvesFromItsSavedManifest() async throws {
        contracts.set(contentHash: Self.appHash, name: baseName.rawValue)
        let fetched = try await makeFetcher().fetchManifest(baseName)
        await makeCache().adopt(fetched)

        let resolved = try await makeCache().manifest(baseName).product

        #expect(!resolved.hasManifest)
        #expect(resolved == fetched.product)
    }

    @Test func cachedManifestNeedsAKnownTldInsteadOfFallingBackToLegacy() async throws {
        publishManifestProduct()
        try await makeCache().adopt(makeFetcher().fetchManifest(baseName))

        await #expect(throws: FailingTldProvider.Unreachable.self) {
            try await self.makeCache(tldProvider: FailingTldProvider()).manifest(self.baseName)
        }
    }

    @Test func fetchNeedsAReadableTldInsteadOfFallingBackToLegacy() async throws {
        publishManifestProduct()

        await #expect(throws: ProductUpdateFetchError.tldUnavailable) {
            try await self.makeFetcher(tldProvider: FailingTldProvider()).fetchManifest(self.baseName)
        }

        #expect(contracts.callCount == 0)
    }

    @Test func openingTheAppDownloadsOnlyTheAppArchive() async throws {
        publishManifestProduct()

        let opened = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        #expect(try opened.contentId == appName)
        #expect(storage.contentExists(contentHash: Self.appHash.toHex()))
        #expect(!storage.contentExists(contentHash: Self.workerHash.toHex()))
    }

    @Test func openedAppOpensAgainWithoutTheChainOrTheGateway() async throws {
        publishManifestProduct()
        let opened = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))
        contracts.failAllReads(with: Unreachable())

        let cached = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        #expect(cached == opened)
        #expect(fetcher.fetchCount == 1)
    }

    @Test func undeclaredKindFailsToOpenWithoutDownloadingAnArchive() async throws {
        publishManifestProduct()

        await #expect(throws: ProductResolutionError.self) {
            try await self.makeLoader().open(self.baseName, strategy: .fixed(.widget), policy: .attempts(1))
        }
        #expect(fetcher.fetchCount == 0)
    }

    @Test func fallbackStrategyOpensAPublishedWidgetAndDownloadsOnlyItsArchive() async throws {
        publishManifestProduct()
        publishWidget()

        let opened = try await makeLoader().open(baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))

        #expect(opened.kind == .widget)
        #expect(try opened.contentId == widgetName)
        #expect(!storage.contentExists(contentHash: Self.appHash.toHex()))
    }

    @Test func fallbackStrategyOpensTheAppWhenNoWidgetIsPublished() async throws {
        publishManifestProduct()

        let opened = try await makeLoader().open(baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))

        #expect(opened.kind == .app)
        #expect(try opened.contentId == appName)
    }

    /// The cache answers from the saved manifest, so a product with only an app is served its app
    /// without the chain instead of being refetched on every open in search of a widget.
    @Test func fallbackStrategyServesTheCachedAppWithoutTheChain() async throws {
        publishManifestProduct()
        let opened = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))
        contracts.failAllReads(with: Unreachable())

        let cached = try await makeLoader().open(baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))

        #expect(cached == opened)
        #expect(fetcher.fetchCount == 1)
    }

    @Test func fallbackStrategyServesTheCachedWidgetWithoutTheChain() async throws {
        publishManifestProduct()
        publishWidget()
        let opened = try await makeLoader().open(baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))
        contracts.failAllReads(with: Unreachable())

        let cached = try await makeLoader().open(baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))

        #expect(cached == opened)
        #expect(cached.kind == .widget)
        #expect(fetcher.fetchCount == 1)
    }

    /// The served kind depends only on what the manifest declares: a declared widget that cannot be
    /// downloaded fails the open rather than quietly running the app in its place.
    @Test func fallbackStrategyFailsWhenAPublishedWidgetCannotBeDownloaded() async throws {
        publishManifestProduct()
        publishWidget()
        let loader = makeLoader(carFetcher: StubCarFetcher(failures: [URLError(.badURL)]))

        await #expect(throws: URLError.self) {
            try await loader.open(self.baseName, strategy: .withAppFallback(.widget), policy: .attempts(1))
        }
        #expect(!storage.contentExists(contentHash: Self.appHash.toHex()))
    }

    @Test func archiveMissingFromDiskIsNotServedFromTheCache() async throws {
        publishManifestProduct()
        _ = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        try storage.deleteAll()

        #expect(await makeCache().appliedArchive(of: appName) == nil)
    }

    @Test func stagedAppArchiveIsNotServedUntilApplied() async throws {
        publishManifestProduct()
        let loader = makeLoader()
        let opened = try await loader.open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        contracts.set(contentHash: Self.updatedAppHash, name: appName.rawValue)
        let update = try #require(await loader.refresh(baseName).first)

        #expect(try await loader.open(baseName, strategy: .fixed(.app), policy: .attempts(1)) == opened)

        await loader.apply(update)

        #expect(try await loader.open(baseName, strategy: .fixed(.app), policy: .attempts(1)).archive == update.archive)
    }

    @Test func republishedWorkerUpdatesOnlyTheWorker() async throws {
        publishManifestProduct()
        _ = try await makeLoader().open(baseName, strategy: .fixed(.app), policy: .attempts(1))
        _ = try await makeLoader().openWorker(baseName, policy: .attempts(1))

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName.rawValue)

        #expect(await makeLoader().refresh(baseName).map(\.contentId) == [workerName])
    }

    @Test func refreshDoesNotDownloadExecutablesNeverOpened() async throws {
        publishManifestProduct()
        let loader = makeLoader()
        _ = try await loader.open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName.rawValue)

        #expect(await loader.refresh(baseName).isEmpty)
        #expect(!storage.contentExists(contentHash: Self.workerHash.toHex()))
        #expect(!storage.contentExists(contentHash: Self.updatedWorkerHash.toHex()))
    }

    @Test func manifestChangeIsSavedWithoutAnUpdate() async throws {
        publishManifestProduct()
        let loader = makeLoader()
        _ = try await loader.open(baseName, strategy: .fixed(.app), policy: .attempts(1))

        publishManifestProduct(displayName: "Renamed shop")

        #expect(await loader.refresh(baseName).isEmpty)
        #expect(try await makeCache().manifest(baseName).product.displayName == "Renamed shop")
    }

    @Test func workerOpenStagesTheWorkerArchiveWithoutTheAppArchive() async throws {
        publishManifestProduct()

        let resolved = try await makeLoader().openWorker(baseName, policy: .attempts(1))

        #expect(resolved.executables.worker?.identifier == workerName)
        #expect(storage.contentExists(contentHash: Self.workerHash.toHex()))
        #expect(!storage.contentExists(contentHash: Self.appHash.toHex()))
        #expect(contentHashCache.getContentHash(name: workerName.rawValue) == Self.workerHash.toHex())
    }

    @Test func workerOpenKeepsTheManifestWorkerWhenItsArchiveFails() async throws {
        publishManifestProduct()

        let resolved = try await makeLoader(carFetcher: StubCarFetcher(failures: [URLError(.timedOut)]))
            .openWorker(baseName, policy: .attempts(1))

        #expect(resolved.executables.worker?.identifier == workerName)
        #expect(contentHashCache.getContentHash(name: workerName.rawValue) == nil)
    }

    @Test func workerOpenFailsWhenTheManifestCannotBeRead() async throws {
        publishManifestProduct()
        contracts.failAllReads(with: Unreachable())

        await #expect(throws: (any Error).self) {
            try await self.makeLoader().openWorker(self.baseName, policy: .attempts(1))
        }
    }

    @Test func openedWorkerOpensAgainWithoutTheNetwork() async throws {
        publishManifestProduct()
        _ = try await makeLoader().openWorker(baseName, policy: .attempts(1))
        contracts.failAllReads(with: Unreachable())

        let resolved = try await makeLoader().openWorker(baseName, policy: .attempts(1))

        #expect(resolved.executables.worker?.identifier == workerName)
        #expect(fetcher.fetchCount == 1)
    }

    @Test func workerOpenRefreshesTheWorkerInTheBackground() async throws {
        publishManifestProduct()
        _ = try await makeLoader().openWorker(baseName, policy: .attempts(1))
        let loader = makeLoader()
        var updates = loader.updates().makeAsyncIterator()

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName.rawValue)
        _ = try await loader.openWorker(baseName, policy: .attempts(1))

        let update = try await updates.next()
        let latestRecord = try await makeFetcher().fetchManifest(baseName).records[workerRecordKey]
        #expect(update == ExecutableRelease(
            contentId: workerName,
            archive: DotNsResolvedContent(
                contentHash: Self.updatedWorkerHash.toHex(),
                localURL: storage.directory(for: Self.updatedWorkerHash.toHex())
            ),
            record: latestRecord
        ))
    }

    @Test func workerOpenStagesAWorkerThatDoesNotServeChat() async throws {
        publishManifestProduct()
        publishWorker(entrypoint: "worker.js", servesChat: false)

        _ = try await makeLoader().openWorker(baseName, policy: .attempts(1))

        #expect(contentHashCache.getContentHash(name: workerName.rawValue) == Self.workerHash.toHex())
    }

    @Test func releaseWhoseRecordCannotBeStoredLeavesTheAppliedArchive() async throws {
        publishManifestProduct()
        _ = try await makeLoader().openWorker(baseName, policy: .attempts(1))
        let unwritable = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data().write(to: unwritable)
        defer { try? FileManager.default.removeItem(at: unwritable) }

        await makeCache(manifestsDirectory: unwritable).apply(ExecutableRelease(
            contentId: workerName,
            archive: DotNsResolvedContent(contentHash: Self.updatedWorkerHash.toHex(), localURL: unwritable),
            record: .value("{}")
        ))

        #expect(contentHashCache.getContentHash(name: workerName.rawValue) == Self.workerHash.toHex())
    }

    @Test func servedExecutableRecordChangesOnlyWithItsArchive() async throws {
        publishManifestProduct()
        let loader = makeLoader()
        _ = try await loader.openWorker(baseName, policy: .attempts(1))

        publishWorker(entrypoint: "worker-v2.js")
        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName.rawValue)
        let update = try #require(await loader.refresh(baseName).first)

        #expect(try await makeCache().manifest(baseName).product.executables.worker?.entrypoint == "worker.js")

        await loader.apply(update)

        #expect(try await makeCache().manifest(baseName).product.executables.worker?.entrypoint == "worker-v2.js")
    }

    @Test func runningWorkerKeepsReadingTheAppliedArchiveUntilTheUpdateIsApplied() async throws {
        publishManifestProduct()
        let loader = makeLoader()
        _ = try await loader.openWorker(baseName, policy: .attempts(1))
        let workerFiles = CompositeProductFileProvider(
            dotNsContentStorage: storage,
            chatScriptStorage: FileChatScriptStorage(),
            contentHashCache: contentHashCache
        )

        contracts.set(contentHash: Self.updatedWorkerHash, name: workerName.rawValue)
        let update = try #require(await loader.refresh(baseName).first)

        #expect(workerFiles.load(for: workerName, relativePath: "worker.js") == Data(Self.workerHash.toHex().utf8))

        await loader.apply(update)

        #expect(workerFiles
            .load(for: workerName, relativePath: "worker.js") == Data(Self.updatedWorkerHash.toHex().utf8))
    }
}

private extension ProductExecutableCacheTests {
    var appName: ProductId { try! productIdExecutable(productId: baseName, kind: .app) }
    var widgetName: ProductId { try! productIdExecutable(productId: baseName, kind: .widget) }
    var workerName: ProductId { try! productIdExecutable(productId: baseName, kind: .worker) }
    var workerRecordKey: ProductManifest.RecordKey {
        ProductManifest.RecordKey(name: workerName, key: ProductManifestRecords.executableKey)
    }

    func publishManifestProduct(displayName: String = "Shop") {
        contracts.set(
            text: #"{"$v":1,"displayName":"\#(displayName)","description":"","icon":{"cid":"bafyicon","format":"png"}}"#,
            name: baseName.rawValue,
            key: ProductManifestRecords.rootKey
        )
        contracts.set(
            text: #"{"$v":1,"kind":"app","appVersion":[1,0,0]}"#,
            name: appName.rawValue,
            key: ProductManifestRecords.executableKey
        )
        publishWorker(entrypoint: "worker.js")
        contracts.set(contentHash: Self.appHash, name: appName.rawValue)
        contracts.set(contentHash: Self.workerHash, name: workerName.rawValue)
    }

    func publishWidget() {
        contracts.set(
            text: #"{"$v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[2],"width":1}}"#,
            name: widgetName.rawValue,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(contentHash: Self.widgetHash, name: widgetName.rawValue)
    }

    func publishWorker(entrypoint: String, servesChat: Bool = true) {
        contracts.set(
            text: #"""
            {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"\#(entrypoint)",
             "includes":{"chat":\#(servesChat),"pocket":\#(!servesChat)}}
            """#,
            name: workerName.rawValue,
            key: ProductManifestRecords.executableKey
        )
    }

    func makeCache(
        tldProvider: DotNsTldProviding = StubTldProvider(resolveError: Unreachable()),
        manifestsDirectory: URL? = nil
    ) -> ProductContentCache {
        ProductContentCache(
            store: ProductManifestFileStore(directory: manifestsDirectory ?? self.manifestsDirectory),
            storage: DotNsLocalStorage(content: storage, contentHashes: contentHashCache),
            tldProvider: tldProvider,
            resolverFactory: ProductResolverFactory(screening: .passingThrough, logger: SilentLogger()),
            logger: SilentLogger()
        )
    }

    func makeFetcher(
        tldProvider: DotNsTldProviding = StubTldProvider(),
        carFetcher: StubCarFetcher? = nil
    ) -> ProductUpdateFetcher {
        ProductUpdateFetcher(
            contractApi: contracts,
            carFetcher: carFetcher ?? fetcher,
            storage: DotNsLocalStorage(content: storage, contentHashes: contentHashCache),
            hostProvider: ProductHostFactory(tldProvider: tldProvider),
            resolverFactory: ProductResolverFactory(screening: .passingThrough, logger: SilentLogger())
        )
    }

    func makeLoader(carFetcher: StubCarFetcher? = nil) -> ProductContentLoader {
        ProductContentLoader(
            fetcher: makeFetcher(carFetcher: carFetcher),
            cache: makeCache(tldProvider: StubTldProvider()),
            environment: ProductContentLoader.Environment(
                pathProvider: StubNetworkPath(),
                backgroundExecutor: StubBackgroundExecutor()
            ),
            timing: ProductContentLoader.Timing(refreshPolicy: .attempts(1)),
            logger: SilentLogger()
        )
    }
}
