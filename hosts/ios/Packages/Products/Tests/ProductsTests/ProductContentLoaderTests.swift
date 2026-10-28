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

    private let versionsDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("ProductContentLoaderTests-\(UUID().uuidString)")
    private let productId = "shop\(UUID().uuidString.prefix(8).lowercased()).dot"
    private let clock = ManualClock()
    private let contracts = StubDotNsContractApi()
    private let storage = InMemoryDotNsContentStorage()
    private let contentHashCache = ContentHashCache()

    init() throws {
        contentHash = try Data.randomOrError(of: 34)
        updatedContentHash = try Data.randomOrError(of: 34)
        contracts.set(contentHash: contentHash, name: productId)
    }

    deinit {
        try? FileManager.default.removeItem(at: versionsDirectory)
        contentHashCache.removeContentHash(name: productId)
    }

    @Test func retriesAnUnreachableChainWithCappedExponentialBackoff() async throws {
        contracts.failNextReads(with: Array(repeating: chainUnreachable, count: 4))
        let environment = try Environment(self)

        let open = Task { try await environment.loader.open(productId, policy: .untilCancelled) }

        for delay in [1, 2, 4, 4] {
            await clock.resumeSleep(for: .seconds(delay))
        }

        #expect(try await open.value.content(for: .app)?.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func retriesAGatewayThatCannotReachTheContentYet() async throws {
        let environment = try Environment(self, fetchFailures: [HTTPResponseError(statusCode: 504)])

        let open = Task { try await environment.loader.open(productId, policy: .untilCancelled) }

        await clock.resumeSleep(for: .seconds(1))

        #expect(try await open.value.content(for: .app)?.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func failsWithoutRetryingWhenTheContractRefusesTheRead() async throws {
        contracts
            .failNextReads(with: [DotNsContractError
                    .contractCallFailed(ReviveContractRevertedError(data: Data([0x08])))])
        let environment = try Environment(self)

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.open(self.productId, policy: .untilCancelled)
        }

        #expect(environment.fetcher.fetchCount == 0)
    }

    @Test func failsWithoutRetryingWhenTheGatewayRejectsTheRequest() async throws {
        let environment = try Environment(self, fetchFailures: [HTTPResponseError(statusCode: 404)])

        await #expect(throws: HTTPResponseError.self) {
            try await environment.loader.open(self.productId, policy: .untilCancelled)
        }

        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func waitsForTheNetworkWithoutSpendingAnAttempt() async throws {
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.notConnectedToInternet)],
            isPathSatisfied: false
        )

        let open = Task { try await environment.loader.open(productId, policy: .attempts(1)) }

        await environment.path.waitForStreamRequests(2)
        environment.path.simulate(isSatisfied: true)

        #expect(try await open.value.content(for: .app)?.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func givesUpOnceThePolicyAttemptsAreSpent() async throws {
        let environment = try Environment(self, fetchFailures: Array(repeating: URLError(.timedOut), count: 3))

        let open = Task { try await environment.loader.open(productId, policy: .attempts(2)) }

        await clock.resumeSleep(for: .seconds(1))

        await #expect(throws: URLError.self) { try await open.value }
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func retriesAnAttemptCutShortByTheBackgroundWindow() async throws {
        let environment = try Environment(self, expiringBackgroundRuns: 1)

        let open = Task { try await environment.loader.open(productId, policy: .untilCancelled) }

        await clock.resumeSleep(for: .seconds(1))

        #expect(try await open.value.content(for: .app)?.localURL == storage.directory(for: contentHash.toHex()))
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func stopsRetryingWhenTheCallerCancels() async throws {
        let environment = try Environment(self, fetchFailures: Array(repeating: URLError(.timedOut), count: 10))

        let open = Task { try await environment.loader.open(productId, policy: .untilCancelled) }

        await clock.waitForSleep(for: .seconds(1))
        open.cancel()

        await #expect(throws: CancellationError.self) { try await open.value }
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func networkOpenReportsResolvingDownloadAndUnpacking() async throws {
        let environment = try Environment(self)
        let phases = PhaseRecorder()

        _ = try await environment.loader.open(productId, policy: .untilCancelled, onPhase: phases.record)

        #expect(phases.recorded == [
            .resolving,
            .downloading(downloadedBytes: StubCarFetcher.downloadedBytes, totalBytes: StubCarFetcher.totalBytes),
            .unpacking
        ])
    }

    @Test func networkOpenReportsWaitingForNetworkAndRetries() async throws {
        let environment = try Environment(
            self,
            fetchFailures: [URLError(.notConnectedToInternet), URLError(.timedOut)],
            isPathSatisfied: false
        )
        let phases = PhaseRecorder()

        let open = Task {
            try await environment.loader.open(self.productId, policy: .untilCancelled, onPhase: phases.record)
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
        _ = try await environment.loader.open(productId, policy: .untilCancelled)
        let phases = PhaseRecorder()

        let cached = try await environment.loader.open(productId, policy: .untilCancelled, onPhase: phases.record)

        #expect(environment.fetcher.fetchCount == 1)
        #expect(phases.recorded.isEmpty)
    }

    @Test func openedProductOpensAgainFromTheCacheWithoutTheNetwork() async throws {
        let environment = try Environment(self)
        let first = try await environment.loader.open(productId, policy: .untilCancelled)

        contracts.failAllReads(with: Unreachable())
        environment.path.simulate(isSatisfied: false)

        let second = try await environment.loader.open(productId, policy: .untilCancelled)

        #expect(environment.fetcher.fetchCount == 1)
        #expect(second.contents == first.contents)
        #expect(second.version == first.version)
    }

    @Test func refreshPublishesAnUpdateOnlyOnceTheNewVersionIsOnDisk() async throws {
        let environment = try Environment(self)
        let opened = try await environment.loader.open(productId, policy: .untilCancelled)
        var updates = environment.loader.updates().makeAsyncIterator()

        contracts.set(contentHash: updatedContentHash, name: productId)
        let update = try #require(await environment.loader.refresh(productId))

        #expect(update.previous == opened.version)
        #expect(update.latest.content(for: .app)?.contentHash == updatedContentHash.toHex())
        #expect(storage.contentExists(contentHash: updatedContentHash.toHex()))
        #expect(try await updates.next() == update)
    }

    @Test func refreshLeavesTheAppliedVersionServedUntilTheUpdateIsApplied() async throws {
        let environment = try Environment(self)
        let opened = try await environment.loader.open(productId, policy: .untilCancelled)

        contracts.set(contentHash: updatedContentHash, name: productId)
        let update = try #require(await environment.loader.refresh(productId))

        #expect(try await environment.loader.open(productId, policy: .untilCancelled).version == opened.version)

        await environment.loader.apply(update)

        #expect(try await environment.loader.open(productId, policy: .untilCancelled).version == update.latest.version)
    }

    @Test func refreshPublishesNothingWhenTheProductIsUnchanged() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, policy: .untilCancelled)

        #expect(await environment.loader.refresh(productId) == nil)
    }

    @Test func pendingUpdateIsNotPublishedAgain() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, policy: .untilCancelled)

        contracts.set(contentHash: updatedContentHash, name: productId)

        #expect(await environment.loader.refresh(productId) != nil)
        #expect(await environment.loader.refresh(productId) == nil)
    }

    @Test func unpublishedProductIsDroppedFromTheCache() async throws {
        let environment = try Environment(self)
        _ = try await environment.loader.open(productId, policy: .untilCancelled)

        contracts.removeContentHash(name: productId)

        #expect(await environment.loader.refresh(productId) == nil)

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.open(self.productId, policy: .attempts(1))
        }
    }
}

private extension ProductContentLoaderTests {
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
            expiringBackgroundRuns: Int = 0
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
                cache: ProductVersionCache(
                    store: ProductVersionFileStore(directory: suite.versionsDirectory),
                    storage: localStorage,
                    tldProvider: StubTldProvider(),
                    resolverFactory: ProductResolverFactory(screening: .passingThrough, logger: SilentLogger())
                ),
                environment: ProductContentLoader.Environment(
                    pathProvider: path,
                    backgroundExecutor: StubBackgroundExecutor(expiringRuns: expiringBackgroundRuns)
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
