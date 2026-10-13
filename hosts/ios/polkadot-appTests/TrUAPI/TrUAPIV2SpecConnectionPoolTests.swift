import Foundation
import Testing
import ChainRegistry
@testable import polkadot_app

struct TrUAPIV2SpecConnectionPoolTests {
    private let genesis = Data(repeating: 0xAA, count: 32)

    @Test func eachConnectGetsItsOwnSocketAndId() throws {
        let sockets = SocketQueue([MockChainSocket(), MockChainSocket()])
        let pool = makePool(sockets: sockets)

        let firstId = try #require(pool.connect(genesisHash: genesis))
        let secondId = try #require(pool.connect(genesisHash: genesis))

        #expect(firstId != secondId)
        #expect(sockets.vended.allSatisfy { $0.openCallCount == 1 })
    }

    @Test func sendWritesFrameVerbatimToOwnSocketOnly() throws {
        let sockets = SocketQueue([MockChainSocket(), MockChainSocket()])
        let pool = makePool(sockets: sockets)
        let firstId = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        let frame = #"{"jsonrpc":"2.0","id":1,"method":"chainHead_v1_follow","params":[true]}"#
        try pool.send(connectionId: firstId, request: frame)

        #expect(sockets.vended[0].sent == [frame])
        #expect(sockets.vended[1].sent.isEmpty)
    }

    @Test func inboundFrameRoutesVerbatimToItsConnection() throws {
        let sockets = SocketQueue([MockChainSocket(), MockChainSocket()])
        let handler = RecordingChainEventHandler()
        let pool = makePool(sockets: sockets, handler: handler)
        _ = try #require(pool.connect(genesisHash: genesis))
        let secondId = try #require(pool.connect(genesisHash: genesis))

        let frame = #"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"s1"}}"#
        sockets.vended[1].simulateReceive(frame)

        #expect(handler.responses.count == 1)
        #expect(handler.responses.first?.0 == secondId)
        #expect(handler.responses.first?.1 == frame)
    }

    @Test func socketDeathReportsCloseOnceAndForgetsConnection() throws {
        let sockets = SocketQueue([MockChainSocket()])
        let handler = RecordingChainEventHandler()
        let pool = makePool(sockets: sockets, handler: handler)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        sockets.vended[0].simulateClose()
        sockets.vended[0].simulateClose(.sleep)
        sockets.vended[0].simulateReceive("late")

        #expect(handler.closed == [connectionId])
        #expect(handler.responses.isEmpty)
        #expect(throws: TrUAPIChainConnectionError.self) {
            try pool.send(connectionId: connectionId, request: "{}")
        }
    }

    @Test func coreCloseIsSilent() throws {
        let sockets = SocketQueue([MockChainSocket()])
        let handler = RecordingChainEventHandler()
        let pool = makePool(sockets: sockets, handler: handler)
        let connectionId = try #require(pool.connect(genesisHash: genesis))

        pool.close(connectionId: connectionId)
        sockets.vended[0].simulateClose()

        #expect(sockets.vended[0].closeCallCount == 1)
        #expect(handler.closed.isEmpty)
    }

    @Test func closeDuringOpenStillReportsTheConnection() throws {
        let socket = MockChainSocket()
        socket.onOpen = { $0.simulateClose(.transport(nil)) }
        let handler = RecordingChainEventHandler()
        let pool = makePool(sockets: SocketQueue([socket]), handler: handler)

        let connectionId = try #require(pool.connect(genesisHash: genesis))

        #expect(handler.closed == [connectionId])
    }

    @Test func unsupportedChainReturnsNil() {
        let pool = makePool(sockets: SocketQueue([]))

        #expect(pool.connect(genesisHash: genesis) == nil)
    }

    @Test func closeAllClosesEverySocketSilently() throws {
        let sockets = SocketQueue([MockChainSocket(), MockChainSocket()])
        let handler = RecordingChainEventHandler()
        let pool = makePool(sockets: sockets, handler: handler)
        _ = try #require(pool.connect(genesisHash: genesis))
        _ = try #require(pool.connect(genesisHash: genesis))

        pool.closeAll()

        #expect(sockets.vended.allSatisfy { $0.closeCallCount == 1 })
        #expect(handler.closed.isEmpty)
    }

    @Test func registryResolverMapsGenesisToChainSocket() throws {
        let chainRegistry = MockChainRegistry()
        let chain = ChainMock.makeChainModel(from: ChainMock.makeRemoteChain(name: "TestChain"), order: 0)
        chainRegistry.chainsByGenesis[genesis.toHex()] = chain
        let pool = TrUAPIV2SpecConnectionPool.makeChainConnections(chainRegistry: chainRegistry, logger: StubLogger())

        #expect(pool.connect(genesisHash: genesis) == nil)

        let socket = MockChainSocket()
        chainRegistry.chainSocketsById[chain.chainId] = socket

        #expect(pool.connect(genesisHash: genesis) != nil)
        #expect(socket.openCallCount == 1)
    }
}

private extension TrUAPIV2SpecConnectionPoolTests {
    final class SocketQueue: @unchecked Sendable {
        private(set) var vended: [MockChainSocket] = []
        private var remaining: [MockChainSocket]

        init(_ sockets: [MockChainSocket]) {
            remaining = sockets
        }

        func next() -> ChainSocketProtocol? {
            guard !remaining.isEmpty else { return nil }
            let socket = remaining.removeFirst()
            vended.append(socket)
            return socket
        }
    }

    func makePool(
        sockets: SocketQueue,
        handler: RecordingChainEventHandler? = nil
    ) -> TrUAPIV2SpecConnectionPool {
        let pool = TrUAPIV2SpecConnectionPool(socketResolver: { _ in sockets.next() }, logger: StubLogger())
        pool.eventHandler = handler
        return pool
    }
}
