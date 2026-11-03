import struct TrUAPIHost.ProductId
import Foundation
import os
import FoundationExt
import Revive
import StructuredConcurrencyTestSupport
import SubstrateSdk
import Testing
@testable import Products

final class ProductContentLoaderTests {
    private struct Unreachable: Error {}

    private let contentHash: Data
    private let updatedContentHash: Data

    private let manifestsDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("ProductContentLoaderTests-\(UUID().uuidString)")
    private let productId = try! ProductId("shop\(UUID().uuidString.prefix(8).lowercased()).dot")
    private let clock = ManualClock()
    private let contracts = StubDotNsContractApi()
    private let storage = InMemoryDotNsContentStorage()
    private let contentHashCache = ContentHashCache()

    init() throws {
        contentHash = try Data.randomOrError(of: 34)
        updatedContentHash = try Data.randomOrError(of: 34)
        contracts.set(contentHash: contentHash, name: productId.rawValue)
    }

    deinit {
        try? FileManager.default.removeItem(at: manifestsDirectory)
        contentHashCache.removeContentHash(name: productId.rawValue)
    }

    @Test func retriesAnUnreachableChainWithCappedExponentialBackoff() async throws {
        contracts.failNextReads(with: Array(repeating: chainUnreachable, count: 4))
        let environment = try Environment(self)

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        for delay in [1, 2, 4, 4] {
            await clock.resumeSleep(for: .seconds(delay))
        }

        #expect(try await open.value.archive.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func retriesAGatewayThatCannotReachTheContentYet() async throws {
        let environment = try Environment(self, fetchFailures: [HTTPResponseError(statusCode: 504)])

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        await clock.resumeSleep(for: .seconds(1))

        #expect(try await open.value.archive.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func workerOpenRetriesAnUnreachableChain() async throws {
        contracts.failNextReads(with: [chainUnreachable])
        let environment = try Environment(self)

        let open = Task { try await environment.loader.openWorker(productId, policy: .attempts(3)) }
        await clock.resumeSleep(for: .seconds(1))

        #expect(try await open.value.id == productId)
    }

    @Test func workerOpenGivesUpOnceTheRefreshAttemptsAreSpent() async throws {
        contracts.failAllReads(with: chainUnreachable)
        let environment = try Environment(self)

        let open = Task { try await environment.loader.openWorker(productId, policy: .attempts(2)) }
        await clock.resumeSleep(for: .seconds(1))

        await #expect(throws: DotNsContractError.self) { try await open.value }
    }

    @Test func workerOpenRetriesTheWorkerDownload() async throws {
        let workerName = "worker.\(productId.rawValue)"
        let workerHash = try Data.randomOrError(of: 34)
        defer { contentHashCache.removeContentHash(name: workerName) }
        contracts.set(
            text: #"{"$v":1,"displayName":"Shop","description":"","icon":{"cid":"bafyicon","format":"png"}}"#,
            name: productId.rawValue,
            key: ProductManifestRecords.rootKey
        )
        contracts.set(
            text: #"""
            {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"worker.js",
             "includes":{"chat":true,"pocket":false}}
            """#,
            name: workerName,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(contentHash: workerHash, name: workerName)
        let environment = try Environment(self, fetchFailures: [HTTPResponseError(statusCode: 504)])

        let open = Task { try await environment.loader.openWorker(productId, policy: .attempts(3)) }
        await clock.resumeSleep(for: .seconds(1))
        _ = try await open.value

        #expect(contentHashCache.getContentHash(name: workerName) == workerHash.toHex())
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func failsWithoutRetryingWhenTheContractRefusesTheRead() async throws {
        contracts
            .failNextReads(with: [DotNsContractError
                    .contractCallFailed(ReviveContractRevertedError(data: Data([0x08])))])
        let environment = try Environment(self)

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .untilCancelled)
        }

        #expect(environment.fetcher.fetchCount == 0)
    }

    @Test func failsWithoutRetryingWhenTheGatewayRejectsTheRequest() async throws {
        let environment = try Environment(self, fetchFailures: [HTTPResponseError(statusCode: 404)])

        await #expect(throws: HTTPResponseError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .untilCancelled)
        }

        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func waitsForTheNetworkWithoutSpendingAnAttempt() async throws {
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.notConnectedToInternet)],
            isPathSatisfied: false
        )

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .attempts(1)) }

        await environment.path.waitForStreamRequests(2)
        environment.path.simulate(isSatisfied: true)

        #expect(try await open.value.archive.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func givesUpOnceThePolicyAttemptsAreSpent() async throws {
        let environment = try Environment(self, fetchFailures: Array(repeating: URLError(.timedOut), count: 3))

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .attempts(2)) }

        await clock.resumeSleep(for: .seconds(1))

        await #expect(throws: URLError.self) { try await open.value }
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func retriesAnAttemptCutShortByTheBackgroundWindow() async throws {
        let environment = try Environment(self, expiringBackgroundRuns: 1)

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        await clock.resumeSleep(for: .seconds(1))

        #expect(try await open.value.archive.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func stopsRetryingWhenTheCallerCancels() async throws {
        let environment = try Environment(self, fetchFailures: Array(repeating: URLError(.timedOut), count: 10))

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        await clock.waitForSleep(for: .seconds(1))
        open.cancel()

        await #expect(throws: CancellationError.self) { try await open.value }
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func rejectsAMissingWidgetEvenWhenTheBaseArchiveExists() async throws {
        let appId = try ProductId("app.\(productId.rawValue)")
        let appContentHash = try Data.randomOrError(of: 34)
        contracts.set(
            text: #"{"$v":1,"displayName":"Shop","description":"A shop","icon":{"cid":"bafyicon","format":"png"}}"#,
            name: productId.rawValue,
            key: ProductManifestRecords.rootKey
        )
        contracts.set(
            text: #"{"$v":1,"kind":"app","appVersion":[1,0,0]}"#,
            name: appId.rawValue,
            key: ProductManifestRecords.executableKey
        )
        contracts.set(contentHash: appContentHash, name: appId.rawValue)
        let environment = try Environment(self)

        await #expect(throws: ProductResolutionError.self) {
            try await environment.loader.open(self.productId, kind: .widget, policy: .attempts(1))
        }
        #expect(environment.fetcher.fetchCount == 0)
    }

    @Test func networkOpenReportsResolvingDownloadAndUnpacking() async throws {
        let environment = try Environment(self)
        let phases = PhaseRecorder()

        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled, onPhase: phases.record)

        #expect(phases.recorded == [
            .resolving,
            .downloading(downloadedBytes: StubCarFetcher.downloadedBytes, totalBytes: StubCarFetcher.totalBytes),
            .unpacking
        ])
    }

    @Test func stagesAWidgetOnlyProductWithoutInventingAnAppArchive() async throws {
        let widgetId = try ProductId("widget.\(productId.rawValue)")
        defer { contentHashCache.removeContentHash(name: widgetId.rawValue) }
        contracts.removeContentHash(name: productId.rawValue)
        contracts.set(
            text: #"""
            {"$v":1,"displayName":"Widget","description":"Widget product",
             "icon":{"cid":"bafyicon","format":"png"}}
            """#,
            name: productId.rawValue,
            key: "manifest"
        )
        contracts.set(
            text: #"{"$v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[1],"width":1}}"#,
            name: widgetId.rawValue,
            key: "executable"
        )
        contracts.set(contentHash: contentHash, name: widgetId.rawValue)
        let environment = try Environment(self)

        let content = try await environment.loader.open(productId, kind: .widget, policy: .attempts(1))

        #expect(content.archive == DotNsResolvedContent(
            contentHash: contentHash.toHex(),
            localURL: storage.directory(for: contentHash.toHex())
        ))
        await #expect(throws: ProductResolutionError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .attempts(1))
        }
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func networkOpenReportsWaitingForNetworkAndRetries() async throws {
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.notConnectedToInternet), URLError(.timedOut)],
            isPathSatisfied: false
        )
        let phases = PhaseRecorder()

        let open = Task {
            try await environment.loader.open(
                self.productId,
                kind: .app,
                policy: .untilCancelled,
                onPhase: phases.record
            )
        }

        await environment.path.waitForStreamRequests(2)
        environment.path.simulate(isSatisfied: true)
        await clock.resumeSleep(for: .seconds(1))
        _ = try await open.value

        #expect(phases.recorded.contains(.waitingForNetwork))
        #expect(phases.recorded.contains(.retrying(attempt: 1)))
    }

    @Test func cachedOpenReportsNoPhases() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)
        let phases = PhaseRecorder()

        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled, onPhase: phases.record)

        #expect(environment.fetcher.fetchCount == 1)
        #expect(phases.recorded.isEmpty)
    }

    @Test func openedProductOpensAgainFromTheCacheWithoutTheNetwork() async throws {
        let environment = try Environment(self)
        let first = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        contracts.failAllReads(with: Unreachable())
        environment.path.simulate(isSatisfied: false)

        let second = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        #expect(environment.fetcher.fetchCount == 1)
        #expect(second == first)
    }

    @Test func refreshPublishesAnUpdateOnlyOnceTheNewArchiveIsOnDisk() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)
        var updates = environment.loader.updates().makeAsyncIterator()

        contracts.set(contentHash: updatedContentHash, name: productId.rawValue)
        let update = try #require(await environment.loader.refresh(productId).first)

        #expect(update.contentId == productId)
        #expect(update.archive?.contentHash == updatedContentHash.toHex())
        #expect(storage.contentExists(contentHash: updatedContentHash.toHex()))
        #expect(try await updates.next() == update)
    }

    @Test func refreshLeavesTheAppliedArchiveServedUntilTheUpdateIsApplied() async throws {
        let environment = try Environment(self)
        let opened = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        contracts.set(contentHash: updatedContentHash, name: productId.rawValue)
        let update = try #require(await environment.loader.refresh(productId).first)

        #expect(try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) == opened)

        await environment.loader.apply(update)

        #expect(try await environment.loader.open(productId, kind: .app, policy: .untilCancelled).archive
            == update.archive)
    }

    @Test func refreshPublishesNothingWhenTheProductIsUnchanged() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        #expect(await environment.loader.refresh(productId).isEmpty)
    }

    @Test func resolvingAnAppliedNameOutsideTheLoaderLeavesItsUpdateToTheLoader() async throws {
        let environment = try Environment(self)
        let opened = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)
        let resolver = DotNsResolver(
            contractApi: contracts,
            carFetcher: StubCarFetcher(),
            contentStorage: storage,
            contentHashCache: contentHashCache
        )

        contracts.set(contentHash: updatedContentHash, name: productId.rawValue)

        #expect(try await resolver.resolveToLocalURL(dotNsName: productId.rawValue) == opened.archive.localURL)
        #expect(await environment.loader.refresh(productId).first?.archive?.contentHash == updatedContentHash.toHex())
    }

    @Test func unpublishedArchiveStaysServedUntilItsRemovalIsApplied() async throws {
        let environment = try Environment(self)
        let opened = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        contracts.removeContentHash(name: productId.rawValue)
        let removal = try #require(await environment.loader.refresh(productId).first)

        #expect(removal == ExecutableRelease(contentId: productId, archive: nil))
        #expect(try await environment.loader.open(productId, kind: .app, policy: .attempts(1)) == opened)

        await environment.loader.apply(removal)

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .attempts(1))
        }
    }

    @Test func stallIsReportedOnceAfterTheFifthFailedAttempt() async throws {
        let reporter = RecordingIssueReporter()
        let environment = try Environment(
            self,
            fetchFailures: Array(repeating: URLError(.timedOut), count: 7),
            reporter: reporter
        )

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        for delay in [1, 2, 4, 4, 4, 4, 4] {
            await clock.resumeSleep(for: .seconds(delay))
        }
        _ = try await open.value

        let issue = try #require(reporter.reported.first)
        #expect(reporter.reported.count == 1)
        #expect(issue.kind == "contentFetchStalled")
        #expect(issue.counters["attempts"] == 5)
        #expect(issue.counters["urlErrorCode"] == URLError.Code.timedOut.rawValue)
    }

    @Test func stallIsReportedOnceTheLoadRunsLongerThanTheThreshold() async throws {
        let reporter = RecordingIssueReporter()
        let start = Date(timeIntervalSince1970: 0)
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.timedOut)],
            reporter: reporter,
            dates: [start, start.addingTimeInterval(61)]
        )

        let open = Task { try await environment.loader.open(productId, kind: .app, policy: .untilCancelled) }

        await clock.resumeSleep(for: .seconds(1))
        _ = try await open.value

        #expect(reporter.reported.map(\.kind) == ["contentFetchStalled"])
        #expect(reporter.reported.first?.counters["elapsedSeconds"] == 61)
    }

    @Test func timeSpentOfflineDoesNotCountTowardsAStall() async throws {
        let reporter = RecordingIssueReporter()
        let start = Date(timeIntervalSince1970: 0)
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.notConnectedToInternet), URLError(.timedOut)],
            isPathSatisfied: false,
            reporter: reporter,
            dates: [start, start.addingTimeInterval(300), start.addingTimeInterval(301)]
        )

        let open = Task { try await environment.loader.open(self.productId, kind: .app, policy: .attempts(1)) }

        await environment.path.waitForStreamRequests(2)
        environment.path.simulate(isSatisfied: true)

        await #expect(throws: URLError.self) { try await open.value }
        #expect(reporter.reported.isEmpty)
    }

    @Test func damagedContentIsReportedAsUnavailable() async throws {
        let reporter = RecordingIssueReporter()
        let environment = try Environment(self, fetchFailures: [CarFetcherError.notCarArchive], reporter: reporter)

        await #expect(throws: CarFetcherError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .untilCancelled)
        }

        let issue = try #require(reporter.reported.first)
        #expect(issue.kind == "contentUnavailable")
        #expect(issue.errorCode == "notCarArchive")
    }

    @Test func productWithNoPublishedContentIsNotReported() async throws {
        let reporter = RecordingIssueReporter()
        contracts.removeContentHash(name: productId.rawValue)
        let environment = try Environment(self, reporter: reporter)

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.open(self.productId, kind: .app, policy: .untilCancelled)
        }

        #expect(reporter.reported.isEmpty)
    }

    @Test func backgroundRefreshFailuresAreNotReported() async throws {
        let reporter = RecordingIssueReporter()
        let environment = try Environment(self, reporter: reporter)
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        contracts.failAllReads(with: DotNsContractError.contractCallFailed(Unreachable()))

        #expect(await environment.loader.refresh(productId).isEmpty)
        #expect(reporter.reported.isEmpty)
    }

    @Test func supersededArchiveIsRemovedOnceNothingUsesIt() async throws {
        let environment = try Environment(self)
        try await applyUpdatedArchive(using: environment)

        await environment.loader.removeSupersededContent(keeping: [])

        #expect(!storage.contentExists(contentHash: contentHash.toHex()))
        #expect(storage.contentExists(contentHash: updatedContentHash.toHex()))
    }

    @Test func pinnedArchiveIsKept() async throws {
        let environment = try Environment(self)
        try await applyUpdatedArchive(using: environment)

        await environment.loader.removeSupersededContent(keeping: [contentHash.toHex()])

        #expect(storage.contentExists(contentHash: contentHash.toHex()))
    }

    @Test func appliedArchiveIsKeptWithoutBeingPinned() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)

        await environment.loader.removeSupersededContent(keeping: [])

        #expect(storage.contentExists(contentHash: contentHash.toHex()))
    }

    @Test func recentlyWrittenArchiveIsKept() async throws {
        let environment = try Environment(self, now: Date(timeIntervalSince1970: 60))
        try await applyUpdatedArchive(using: environment)

        await environment.loader.removeSupersededContent(keeping: [])

        #expect(storage.contentExists(contentHash: contentHash.toHex()))
    }
}

private extension ProductContentLoaderTests {
    func applyUpdatedArchive(using environment: Environment) async throws {
        _ = try await environment.loader.open(productId, kind: .app, policy: .untilCancelled)
        contracts.set(contentHash: updatedContentHash, name: productId.rawValue)
        let update = try #require(await environment.loader.refresh(productId).first)
        await environment.loader.apply(update)
    }

    var chainUnreachable: Error {
        DotNsContractError.contractCallFailed(Unreachable())
    }

    struct Environment {
        let fetcher: StubCarFetcher
        let path: StubNetworkPath
        let loader: ProductContentLoader

        init(
            _ suite: ProductContentLoaderTests,
            fetchFailures: [Error] = [],
            isPathSatisfied: Bool = true,
            expiringBackgroundRuns: Int = 0,
            reporter: RecordingIssueReporter = RecordingIssueReporter(),
            dates: [Date] = [Date(timeIntervalSince1970: 0)],
            now: Date = Date()
        ) throws {
            fetcher = StubCarFetcher(failures: fetchFailures)
            path = StubNetworkPath(isSatisfied: isPathSatisfied)

            let localStorage = DotNsLocalStorage(content: suite.storage, contentHashes: suite.contentHashCache)

            loader = ProductContentLoader(
                fetcher: ProductUpdateFetcher(
                    contractApi: suite.contracts,
                    carFetcher: fetcher,
                    storage: localStorage,
                    hostProvider: ProductHostFactory(tldProvider: StubTldProvider()),
                    resolverFactory: ProductResolverFactory(screening: .passingThrough, logger: SilentLogger())
                ),
                cache: ProductContentCache(
                    store: ProductManifestFileStore(directory: suite.manifestsDirectory),
                    storage: localStorage,
                    tldProvider: StubTldProvider(),
                    resolverFactory: ProductResolverFactory(screening: .passingThrough, logger: SilentLogger()),
                    logger: SilentLogger()
                ),
                environment: ProductContentLoader.Environment(
                    pathProvider: path,
                    backgroundExecutor: StubBackgroundExecutor(expiringRuns: expiringBackgroundRuns),
                    diagnostics: ProductLoadDiagnostics(reporter: reporter, dateProvider: SequencedDateProvider(dates)),
                    dateProvider: SequencedDateProvider([now])
                ),
                timing: ProductContentLoader.Timing(
                    maxBackoff: .seconds(4),
                    refreshPolicy: .attempts(1),
                    clock: suite.clock
                ),
                logger: SilentLogger()
            )
        }
    }
}

private final class PhaseRecorder: @unchecked Sendable {
    private let phases = OSAllocatedUnfairLock(initialState: [ProductLoadPhase]())

    var recorded: [ProductLoadPhase] { phases.withLock { $0 } }

    var record: ProductLoadPhaseHandler {
        { [phases] phase in phases.withLock { $0.append(phase) } }
    }
}
