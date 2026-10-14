import Foundation
import SubstrateSdk
import Testing

@testable import SubstrateOperation

@Suite("ViewFunctionFetcher caches view-function results")
struct ViewFunctionFetcherTests {
    @Test("Repeated fetch of the same key within TTL calls the executor once")
    func cachesResultWithinTtl() async throws {
        let spy = SpyViewFunctionExecutor()
        let fetcher = ViewFunctionFetcher(executor: spy, chainId: Self.chain)

        let first: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)
        let second: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)

        #expect(first == 1)
        #expect(second == 1)
        #expect(spy.callCount == 1)
    }

    @Test("Fetch re-executes after the TTL window elapses")
    func refetchesAfterTtlExpires() async throws {
        let clock = MutableClock(now: Date(timeIntervalSince1970: 0))
        let spy = SpyViewFunctionExecutor()
        let fetcher = ViewFunctionFetcher(executor: spy, chainId: Self.chain, ttl: 100, dateProvider: clock)

        let first: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)

        clock.set(Date(timeIntervalSince1970: 50))
        let withinWindow: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)

        clock.set(Date(timeIntervalSince1970: 150))
        let afterExpiry: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)

        #expect(first == 1)
        #expect(withinWindow == 1)
        #expect(afterExpiry == 2)
        #expect(spy.callCount == 2)
    }

    @Test("Different function names are cached independently")
    func cachesPerFunction() async throws {
        let spy = SpyViewFunctionExecutor()
        let fetcher = ViewFunctionFetcher(executor: spy, chainId: Self.chain)

        let a: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA)
        let b: UInt32 = try await fetcher.fetch(viewFunction: Self.functionB)

        #expect(a != b)
        #expect(spy.callCount == 2)
    }

    @Test("The same function with different args is cached independently")
    func cachesPerArgs() async throws {
        let spy = SpyViewFunctionExecutor()
        let fetcher = ViewFunctionFetcher(executor: spy, chainId: Self.chain)

        let a: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA, args: [RawArg(0x01)])
        let b: UInt32 = try await fetcher.fetch(viewFunction: Self.functionA, args: [RawArg(0x02)])

        #expect(a != b)
        #expect(spy.callCount == 2)
    }

    @Test("Concurrent identical fetches invoke the executor once")
    func coalescesConcurrentIdenticalFetches() async throws {
        let spy = SpyViewFunctionExecutor()
        let fetcher = ViewFunctionFetcher(executor: spy, chainId: Self.chain)

        async let first: UInt32 = fetcher.fetch(viewFunction: Self.functionA)
        async let second: UInt32 = fetcher.fetch(viewFunction: Self.functionA)

        let results = try await [first, second]

        #expect(results == [1, 1])
        #expect(spy.callCount == 1)
    }

    private static let chain: ChainId = "chain"
    private static let functionA = ViewFunctionCodingPath(moduleName: "Coinage", functionName: "get_a")
    private static let functionB = ViewFunctionCodingPath(moduleName: "Coinage", functionName: "get_b")
}
