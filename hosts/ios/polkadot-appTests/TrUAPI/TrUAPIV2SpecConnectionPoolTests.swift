import Foundation
import Testing
import ChainRegistry
import IssueMonitoring
import os
import SDKLogger
@testable import polkadot_app

struct TrUAPIV2SpecConnectionPoolTests {
    private let genesis = Data(repeating: 0xAA, count: 32)

    /// The Android host logs a transport death as a failure and anything else as a close, so both
    /// hosts read the same in Sentry.
    @Test func transportDeathLogsAsFailureAndOtherClosesAsClose() throws {
        struct Refused: Error {}
        let url = try #require(URL(string: "wss://node.example"))

        #expect(
            TrUAPIV2SpecConnectionPool.closeLine(connectionId: 3, url: url, reason: .transport(Refused()))
                == "chain connection 3 failed on wss://node.example: Refused()"
        )
        #expect(
            TrUAPIV2SpecConnectionPool.closeLine(connectionId: 3, url: url, reason: .peerClosed)
                == "chain connection 3 closed on wss://node.example: peerClosed"
        )
    }

    /// Each open line names the connection its socket belongs to, so one connection reads end to end in Sentry.
    @Test func anOpenedSocketLogsItsOwnConnection() throws {
        let logger = RecordingLogger()
        let pool = makePool(registry: makeRegistry(serving: [MockChainSocket(), MockChainSocket()]), logger: logger)

        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        #expect(logger.lines == [
            .info("[truapi.chain] chain connection 1 dialling wss://node.example"),
            .info("[truapi.chain] chain connection 1 open on wss://node.example"),
            .info("[truapi.chain] chain connection 2 dialling wss://node.example"),
            .info("[truapi.chain] chain connection 2 open on wss://node.example")
        ])
    }

    /// A dead node is routine, so the pool tells the diagnostic about every dial that died or timed out before
    /// opening and every socket that did open, and leaves the threshold to it. A close the peer or the pool
    /// asked for is not a failure.
    @Test func failedDialsAreFailuresAndOpensRecoveries() throws {
        struct Refused: Error {}
        let sockets = [MockChainSocket(), MockChainSocket(), MockChainSocket()]
        sockets[0].completesOpen = false
        sockets[1].completesOpen = false
        let diagnostic = RecordingIssueDiagnostic()
        let pool = makePool(registry: makeRegistry(serving: sockets), issueDiagnostic: diagnostic)
        for _ in sockets { _ = try #require(pool.connect(genesisHash: genesis)) }

        sockets[0].simulateClose(.transport(Refused()))
        sockets[1].simulateClose(.connectTimeout)
        sockets[2].simulateClose(.peerClosed)

        #expect(diagnostic.failureCount == 2)
        #expect(diagnostic.recoveryCount == 1)
    }

    /// One network blip kills every open socket with a transport error at once, which says nothing about the
    /// node, so sockets that had opened never count towards the threshold.
    @Test func transportDeathsOfOpenedSocketsAreNotFailures() throws {
        struct Dropped: Error {}
        let sockets = [MockChainSocket(), MockChainSocket(), MockChainSocket()]
        let diagnostic = RecordingIssueDiagnostic()
        let pool = makePool(registry: makeRegistry(serving: sockets), issueDiagnostic: diagnostic)
        for _ in sockets { _ = try #require(pool.connect(genesisHash: genesis)) }

        sockets.forEach { $0.simulateClose(.transport(Dropped())) }

        #expect(diagnostic.failureCount == 0)
    }

    /// Dials that die before opening are the failures the threshold exists for. Each reconnect dials the chain's
    /// next node, so three failures in a row land on three nodes and still trip it.
    @Test func transportDeathsOfNeverOpenedSocketsOnRotatingNodesTripTheThreshold() throws {
        struct Refused: Error {}
        let sockets = try nodeSockets(count: 3)
        sockets.forEach { $0.completesOpen = false }
        let reporter = RecordingIssueReporter()
        let diagnostic = makeThresholdDiagnostic(reporter)
        let pool = makePool(registry: makeRegistry(serving: sockets), issueDiagnostic: diagnostic)
        for _ in sockets { _ = try #require(pool.connect(genesisHash: genesis)) }

        sockets[0].simulateClose(.transport(Refused()))
        sockets[1].simulateClose(.transport(Refused()))
        #expect(reporter.issues.isEmpty)

        sockets[2].simulateClose(.transport(Refused()))
        #expect(Set(sockets.map(\.url)).count == 3)
        #expect(reporter.issues.map(\.kind) == ["rpcConnectionFailing"])
    }

    /// Any node of the chain opening ends the run of failures for the whole chain, not only for that node, so
    /// two later failures on the other nodes start a new count rather than finishing the old one.
    @Test func anyNodeOpeningClearsTheChainsFailures() throws {
        struct Refused: Error {}
        let sockets = try nodeSockets(count: 5)
        let reporter = RecordingIssueReporter()
        let diagnostic = makeThresholdDiagnostic(reporter)
        let pool = makePool(registry: makeRegistry(serving: sockets), issueDiagnostic: diagnostic)

        // Dialled in turn: nodes 0 and 1 fail, node 2 opens, nodes 3 and 4 fail.
        for (node, socket) in sockets.enumerated() {
            socket.completesOpen = node == 2
            _ = try #require(pool.connect(genesisHash: genesis))
            if node != 2 { socket.simulateClose(node.isMultiple(of: 2) ? .transport(Refused()) : .connectTimeout) }
        }

        #expect(reporter.issues.isEmpty)
    }

    /// Two chains keep separate counts, so failures on one never trip the other.
    @Test func eachChainCountsItsOwnFailures() throws {
        struct Refused: Error {}
        let otherGenesis = Data(repeating: 0xBB, count: 32)
        let sockets = try nodeSockets(count: 2)
        let otherSockets = try nodeSockets(count: 1)
        (sockets + otherSockets).forEach { $0.completesOpen = false }
        let registry = makeRegistry(serving: sockets)
        let otherChain = register(genesis: otherGenesis, in: registry)
        registry.enqueueChainSockets(otherSockets, for: otherChain.chainId)
        let reporter = RecordingIssueReporter()
        let pool = makePool(registry: registry, issueDiagnostic: makeThresholdDiagnostic(reporter))
        for _ in sockets { _ = try #require(pool.connect(genesisHash: genesis)) }
        _ = try #require(pool.connect(genesisHash: otherGenesis))

        (sockets + otherSockets).forEach { $0.simulateClose(.transport(Refused())) }

        #expect(reporter.issues.isEmpty)
    }

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

private final class RecordingIssueReporter: IssueReporting, @unchecked Sendable {
    private let recorded = OSAllocatedUnfairLock<[CriticalIssue]>(initialState: [])

    var issues: [CriticalIssue] { recorded.withLock { $0 } }

    func report(_ issue: CriticalIssue) {
        recorded.withLock { $0.append(issue) }
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

    /// Sockets on distinct nodes, as the registry hands out one per attempt.
    func nodeSockets(count: Int) throws -> [MockChainSocket] {
        try (0 ..< count).map { try MockChainSocket(url: #require(URL(string: "wss://node-\($0).example"))) }
    }

    func makeThresholdDiagnostic(_ reporter: IssueReporting) -> IssueDiagnostic {
        ThresholdIssueDiagnostic(flow: "productLoad", kind: "rpcConnectionFailing", threshold: 3, reporter: reporter)
    }

    func register(genesis: Data, in registry: MockChainRegistry) -> ChainModel {
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "TestChain"), order: 0)
        registry.chainsByGenesis[genesis.toHex()] = chain
        return chain
    }

    func makePool(
        registry: MockChainRegistry,
        handler: RecordingChainEventHandler? = nil,
        lifecycle: Lifecycle = Lifecycle(),
        issueDiagnostic: IssueDiagnostic = NoopIssueDiagnostic(),
        logger: SDKLoggerProtocol = StubLogger()
    ) -> TrUAPIV2SpecConnectionPool {
        let pool = TrUAPIV2SpecConnectionPool(
            chainRegistry: registry,
            applicationHandler: lifecycle.applicationHandler,
            dateProvider: lifecycle.dateProvider,
            resumePolicy: .init(expiryThreshold: 30, probeTimeout: 5),
            issueDiagnostic: issueDiagnostic,
            logger: logger
        )
        pool.eventHandler = handler
        return pool
    }
}
