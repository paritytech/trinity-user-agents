import Foundation
import FoundationExt
import os
import Revive
import StructuredConcurrencyTestSupport
import SubstrateSdk
import Testing
@testable import Products

struct ProductContentLoaderTests {
    private struct ChainUnreachable: Error {}

    private static let resolver = EvmAddress(repeating: 0xF1, count: EvmAddressFormat.size)
    private static let productId = "shop.dot"
    private static let contentHashHex = Data(repeating: 0xCD, count: 34).toHex()

    @Test func retriesAnUnreachableChainWithCappedExponentialBackoff() async throws {
        let environment = Environment(chainFailures: Array(repeating: ChainUnreachable(), count: 4))

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled) }

        for delay in [1, 2, 4, 4] {
            await environment.clock.resumeSleep(for: .seconds(delay))
        }

        #expect(try await load.value == environment.storage.directory(for: Self.contentHashHex))
        #expect(environment.contracts.calls(to: Self.resolver).count == 5)
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func retriesAGatewayThatCannotReachTheContentYet() async throws {
        let environment = Environment(fetchFailures: [HTTPResponseError(statusCode: 504)])

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled) }

        await environment.clock.resumeSleep(for: .seconds(1))

        #expect(try await load.value == environment.storage.directory(for: Self.contentHashHex))
        #expect(environment.fetcher.fetchCount == 2)
        #expect(environment.storage.savedHashes == [Self.contentHashHex])
    }

    @Test func failsWithoutRetryingWhenTheContractRefusesTheRead() async throws {
        let environment = Environment(chainFailures: [ReviveContractRevertedError(data: Data([0x08]))])

        await #expect(throws: DotNsContractError.self) {
            try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled)
        }

        #expect(environment.contracts.calls(to: Self.resolver).count == 1)
        #expect(environment.fetcher.fetchCount == 0)
    }

    @Test func failsWithoutRetryingWhenTheGatewayRejectsTheRequest() async throws {
        let environment = Environment(fetchFailures: [HTTPResponseError(statusCode: 404)])

        await #expect(throws: HTTPResponseError.self) {
            try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled)
        }

        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func waitsForTheNetworkWithoutSpendingAnAttempt() async throws {
        let environment = Environment(fetchFailures: [URLError(.notConnectedToInternet)], isPathSatisfied: false)

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .attempts(1)) }

        await environment.path.waitForStreamRequests(2)
        environment.path.simulate(isSatisfied: true)

        #expect(try await load.value == environment.storage.directory(for: Self.contentHashHex))
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func givesUpOnceThePolicyAttemptsAreSpent() async throws {
        let environment = Environment(fetchFailures: Array(repeating: URLError(.timedOut), count: 3))

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .attempts(2)) }

        await environment.clock.resumeSleep(for: .seconds(1))

        await #expect(throws: URLError.self) { try await load.value }
        #expect(environment.fetcher.fetchCount == 2)
    }

    @Test func retriesAnAttemptCutShortByTheBackgroundWindow() async throws {
        let environment = Environment(expiringBackgroundRuns: 1)

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled) }

        await environment.clock.resumeSleep(for: .seconds(1))

        #expect(try await load.value == environment.storage.directory(for: Self.contentHashHex))
        #expect(environment.fetcher.fetchCount == 1)
    }

    @Test func stopsRetryingWhenTheCallerCancels() async throws {
        let environment = Environment(fetchFailures: Array(repeating: URLError(.timedOut), count: 10))

        let load = Task { try await environment.loader.loadAppContent(of: Self.productId, policy: .untilCancelled) }

        await environment.clock.waitForSleep(for: .seconds(1))
        load.cancel()

        await #expect(throws: CancellationError.self) { try await load.value }
        #expect(environment.fetcher.fetchCount == 1)
    }
}

private extension ProductContentLoaderTests {
    struct Environment {
        let clock = ManualClock()
        let contracts: StubReviveContractApi
        let fetcher: StubCarFetcher
        let storage = InMemoryDotNsContentStorage()
        let path: StubNetworkPath
        let loader: ProductContentLoader

        init(
            chainFailures: [Error] = [],
            fetchFailures: [Error] = [],
            isPathSatisfied: Bool = true,
            expiringBackgroundRuns: Int = 0
        ) {
            contracts = StubReviveContractApi(answers: [
                ProductContentLoaderTests.resolver: Self.contentHashAnswer(failures: chainFailures)
            ])
            fetcher = StubCarFetcher(failures: fetchFailures)
            path = StubNetworkPath(isSatisfied: isPathSatisfied)

            let config = DotNsConfig(
                contractsChainId: "asset-hub",
                resolverContractAddress: ProductContentLoaderTests.resolver,
                nameRegistryContractAddress: nil,
                ipfsGatewayBaseUrl: URL(fileURLWithPath: "/ipfs")
            )

            let dotNsResolver = DotNsResolver(
                contractApi: ReviveDotNsContractApi(contractApi: contracts) { config },
                carFetcher: fetcher,
                contentStorage: storage,
                contentHashCache: ContentHashCache()
            )

            loader = ProductContentLoader(
                productResolver: ProductResolver(
                    dotNsResolver: dotNsResolver,
                    hostProvider: ProductHostFactory(tldProvider: StubTldProvider()),
                    logger: SilentLogger()
                ),
                dotNsResolver: dotNsResolver,
                pathProvider: path,
                backgroundExecutor: StubBackgroundExecutor(expiringRuns: expiringBackgroundRuns),
                timing: ProductContentLoader.Timing(maxBackoff: .seconds(4), clock: clock)
            )
        }

        static func contentHashAnswer(failures: [Error]) -> @Sendable (Data) throws -> Data {
            let remaining = OSAllocatedUnfairLock(initialState: failures)

            return { _ in
                if let failure = remaining.withLock({ $0.isEmpty ? nil : $0.removeFirst() }) {
                    throw failure
                }

                return AbiOutput.dynamicBytes(Eip1577.ipfsPrefix + Data(repeating: 0xCD, count: 34))
            }
        }
    }
}
