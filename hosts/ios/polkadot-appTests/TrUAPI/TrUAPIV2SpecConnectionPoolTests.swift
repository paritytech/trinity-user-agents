import Foundation
import Testing
import ChainRegistry
@testable import polkadot_app

struct TrUAPIV2SpecConnectionPoolTests {
    private let genesis = Data(repeating: 0xAA, count: 32)

    @Test func eachConnectGetsItsOwnSocketAndId() throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let pool = makePool(registry: makeRegistry(serving: sockets))

        let firstId = try #require(pool.connect(genesisHash: genesis))
        let secondId = try #require(pool.connect(genesisHash: genesis))

        #expect(firstId != secondId)
        #expect(sockets.allSatisfy { $0.openCallCount == 1 })
    }

    @Test func sendWritesFrameVerbatimToOwnSocketOnly() throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let pool = makePool(registry: makeRegistry(serving: sockets))
        let firstId = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        let frame = #"{"jsonrpc":"2.0","id":1,"method":"chainHead_v1_follow","params":[true]}"#
        try pool.send(connectionId: firstId, request: frame)

        #expect(sockets[0].sent == [frame])
        #expect(sockets[1].sent.isEmpty)
    }

    @Test func inboundFrameRoutesVerbatimToItsConnection() throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let handler = RecordingChainEventHandler()
        let pool = makePool(registry: makeRegistry(serving: sockets), handler: handler)
        _ = try #require(pool.connect(genesisHash: genesis))
        let secondId = try #require(pool.connect(genesisHash: genesis))

        let frame = #"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"s1"}}"#
        sockets[1].simulateReceive(frame)

        #expect(handler.responses.count == 1)
        #expect(handler.responses.first?.0 == secondId)
        #expect(handler.responses.first?.1 == frame)
    }

    @Test func socketDeathReportsCloseOnceAndForgetsConnection() throws {
        let socket = MockChainSocket()
        let handler = RecordingChainEventHandler()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        socket.simulateClose()
        socket.simulateClose(.resumed)
        socket.simulateReceive("late")

        #expect(handler.closed == [connectionId])
        #expect(handler.responses.isEmpty)
        #expect(throws: TrUAPIChainConnectionError.self) {
            try pool.send(connectionId: connectionId, request: "{}")
        }
    }

    @Test func coreCloseIsSilent() throws {
        let socket = MockChainSocket()
        let handler = RecordingChainEventHandler()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        pool.close(connectionId: connectionId)
        socket.simulateClose()

        #expect(socket.closeCallCount == 1)
        #expect(handler.closed.isEmpty)
    }

    @Test func closeDuringOpenStillReportsTheConnection() throws {
        let socket = MockChainSocket()
        socket.onOpen = { $0.simulateClose(.transport(nil)) }
        let handler = RecordingChainEventHandler()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler)

        let connectionId = try #require(pool.connect(genesisHash: genesis))

        #expect(handler.closed == [connectionId])
    }

    @Test func unknownGenesisReturnsNil() {
        let registry = MockChainRegistry()
        let pool = makePool(registry: registry)

        #expect(pool.connect(genesisHash: genesis) == nil)
        #expect(registry.chainSocketRequests.isEmpty)
    }

    @Test func chainWithoutSocketReturnsNil() {
        let pool = makePool(registry: makeRegistry(serving: []))

        #expect(pool.connect(genesisHash: genesis) == nil)
    }

    @Test func closeAllClosesEverySocketSilently() throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let handler = RecordingChainEventHandler()
        let pool = makePool(registry: makeRegistry(serving: sockets), handler: handler)
        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        pool.closeAll()

        #expect(sockets.allSatisfy { $0.closeCallCount == 1 })
        #expect(handler.closed.isEmpty)
    }

    @Test func nodeAttemptGrowsPerChain() throws {
        let otherGenesis = Data(repeating: 0xBB, count: 32)
        let registry = makeRegistry(serving: [MockChainSocket(), MockChainSocket()])
        let otherChain = register(genesis: otherGenesis, in: registry)
        registry.enqueueChainSockets([MockChainSocket()], for: otherChain.chainId)
        let pool = makePool(registry: registry)

        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: otherGenesis))

        let chainId = try #require(registry.chainsByGenesis[genesis.toHex()]?.chainId)
        #expect(registry.chainSocketRequests.map(\.attempt) == [0, 1, 0])
        #expect(registry.chainSocketRequests.map(\.chainId) == [chainId, chainId, otherChain.chainId])
    }

    // MARK: - Foreground revalidation

    @Test func shortBackgroundProbesEverySocket() async throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: sockets), lifecycle: lifecycle)
        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        await lifecycle.dateProvider.advance(by: 29)
        await lifecycle.foreground(pool: pool)

        #expect(sockets.allSatisfy { $0.probeTimeouts == [5] && $0.expireCallCount == 0 })
    }

    @Test func longBackgroundExpiresEverySocket() async throws {
        let sockets = [MockChainSocket(), MockChainSocket()]
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: sockets), lifecycle: lifecycle)
        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        await lifecycle.dateProvider.advance(by: 30)
        await lifecycle.foreground(pool: pool)

        #expect(sockets.allSatisfy { $0.expireCallCount == 1 && $0.probeTimeouts.isEmpty })
    }

    @Test func expiredSocketIsReportedClosed() async throws {
        let socket = MockChainSocket()
        let handler = RecordingChainEventHandler()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler, lifecycle: lifecycle)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        await lifecycle.dateProvider.advance(by: 60)
        await lifecycle.foreground(pool: pool)
        socket.simulateClose(.resumed)

        #expect(handler.closed == [connectionId])
    }

    @Test func backgroundDoesNotRevalidate() async throws {
        let socket = MockChainSocket()
        let handler = RecordingChainEventHandler()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler, lifecycle: lifecycle)
        _ = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        await lifecycle.dateProvider.advance(by: 600)

        #expect(socket.expireCallCount == 0)
        #expect(socket.probeTimeouts.isEmpty)
        #expect(socket.closeCallCount == 0)
        #expect(handler.closed.isEmpty)
    }

    @Test func transportDeathWhileBackgroundedIsReported() async throws {
        let socket = MockChainSocket()
        let handler = RecordingChainEventHandler()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), handler: handler, lifecycle: lifecycle)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        socket.simulateClose(.peerClosed)

        #expect(handler.closed == [connectionId])
    }

    @Test func becomingActiveWithoutBackgroundRevalidatesNothing() async throws {
        let socket = MockChainSocket()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), lifecycle: lifecycle)
        _ = try #require(pool.connect(genesisHash: genesis))

        lifecycle.applicationHandler.simulateDidBecomeActive()
        await lifecycle.foreground(pool: pool)

        #expect(socket.expireCallCount == 0)
        #expect(socket.probeTimeouts.isEmpty)
    }

    @Test func socketReleasedByCoreIsNotRevalidated() async throws {
        let socket = MockChainSocket()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), lifecycle: lifecycle)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        await lifecycle.background(pool: pool)
        pool.close(connectionId: connectionId)
        await lifecycle.dateProvider.advance(by: 60)
        await lifecycle.foreground(pool: pool)

        #expect(socket.expireCallCount == 0)
    }

    @Test func socketOpenedWhileBackgroundedIsRevalidated() async throws {
        let socket = MockChainSocket()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), lifecycle: lifecycle)

        await lifecycle.background(pool: pool)
        _ = try #require(pool.connect(genesisHash: genesis))
        await lifecycle.dateProvider.advance(by: 1)
        await lifecycle.foreground(pool: pool)

        #expect(socket.probeTimeouts == [5])
    }

    @Test func backToBackEventsKeepBackgroundBeforeForeground() async throws {
        let socket = MockChainSocket()
        let lifecycle = Lifecycle()
        let pool = makePool(registry: makeRegistry(serving: [socket]), lifecycle: lifecycle)
        _ = try #require(pool.connect(genesisHash: genesis))

        lifecycle.applicationHandler.simulateDidEnterBackground()
        lifecycle.applicationHandler.simulateWillEnterForeground()
        await pool.awaitLifecycleEvents()

        #expect(socket.probeTimeouts == [5])
    }
}

private extension TrUAPIV2SpecConnectionPoolTests {
    struct Lifecycle {
        let applicationHandler = MockApplicationHandler()
        let dateProvider = MockDateProvider()

        func background(pool: TrUAPIV2SpecConnectionPool) async {
            applicationHandler.simulateDidEnterBackground()
            await pool.awaitLifecycleEvents()
        }

        func foreground(pool: TrUAPIV2SpecConnectionPool) async {
            applicationHandler.simulateWillEnterForeground()
            await pool.awaitLifecycleEvents()
        }
    }

    /// A registry that resolves `genesis` to a test chain and hands out `sockets` for it in order.
    func makeRegistry(serving sockets: [MockChainSocket]) -> MockChainRegistry {
        let registry = MockChainRegistry()
        let chain = register(genesis: genesis, in: registry)
        registry.enqueueChainSockets(sockets, for: chain.chainId)
        return registry
    }

    func register(genesis: Data, in registry: MockChainRegistry) -> ChainModel {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "TestChain"), order: 0)
        registry.chainsByGenesis[genesis.toHex()] = chain
        return chain
    }

    func makePool(
        registry: MockChainRegistry,
        handler: RecordingChainEventHandler? = nil,
        lifecycle: Lifecycle = Lifecycle()
    ) -> TrUAPIV2SpecConnectionPool {
        let pool = TrUAPIV2SpecConnectionPool(
            chainRegistry: registry,
            applicationHandler: lifecycle.applicationHandler,
            dateProvider: lifecycle.dateProvider,
            resumePolicy: .init(expiryThreshold: 30, probeTimeout: 5),
            logger: StubLogger()
        )
        pool.eventHandler = handler
        return pool
    }
}
