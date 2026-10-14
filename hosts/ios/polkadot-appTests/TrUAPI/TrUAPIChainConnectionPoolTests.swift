import Foundation
import Testing
import ChainRegistry
import SubstrateSdk
@testable import polkadot_app

struct TrUAPIChainConnectionPoolTests {
    private let genesis = Data(repeating: 0xAA, count: 32)

    private func makeChainRegistry() -> MockChainRegistry {
        let chainRegistry = MockChainRegistry()
        let remoteChain = ChainMock.makeRemoteChain(name: "TestChain")
        chainRegistry.chainsByGenesis[genesis.toHex()] = ChainMock.makeChainModel(from: remoteChain, order: 0)
        return chainRegistry
    }

    /// The resolver shape the app injects: genesis → registry chain → live connection.
    private func makeRegistryResolver(
        chainRegistry: ChainRegistryProtocol
    ) -> TrUAPIChainConnectionPool.EngineResolver {
        { genesisHash in
            chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                chainRegistry.getConnection(for: chain.chainId)
            }
        }
    }

    @Test func twoPoolsSameChainShareOneEngine() throws {
        let shared = MockJSONRPCEngine()
        let resolver: TrUAPIChainConnectionPool.EngineResolver = { _ in shared }
        let handlerA = RecordingChainEventHandler()
        let handlerB = RecordingChainEventHandler()

        let poolA = TrUAPIChainConnectionPool(engineResolver: resolver, logger: StubLogger())
        let poolB = TrUAPIChainConnectionPool(engineResolver: resolver, logger: StubLogger())
        poolA.eventHandler = handlerA
        poolB.eventHandler = handlerB

        let idA = try #require(poolA.connect(genesisHash: genesis))
        let idB = try #require(poolB.connect(genesisHash: genesis))

        // Same physical engine, colliding core ids — no interference.
        try poolA.send(connectionId: idA, request: #"{"jsonrpc":"2.0","id":1,"method":"m","params":[]}"#)
        try poolB.send(connectionId: idB, request: #"{"jsonrpc":"2.0","id":1,"method":"m","params":[]}"#)
        #expect(shared.calls.count == 2)

        // Complete pool B's call only; response reaches only handler B, id restored.
        let engineIds = shared.calls.keys.sorted()
        shared.calls[engineIds[1]]?.completion(.success(.stringValue("forB")))
        #expect(handlerA.responses.isEmpty)
        #expect(handlerB.responses.count == 1)
        #expect(handlerB.responses[0].0 == idB)
        #expect(handlerB.responses[0].1.contains(#""id":1"#))
        #expect(handlerB.responses[0].1.contains("forB"))
    }

    @Test func unknownConnectionThrows() {
        let pool = TrUAPIChainConnectionPool(engineResolver: { _ in nil }, logger: StubLogger())
        #expect {
            try pool.send(connectionId: 9, request: "{}")
        } throws: { error in
            guard case TrUAPIChainConnectionError.unknownConnection(9) = error else {
                return false
            }
            return true
        }
    }

    @Test func unresolvedEngineReturnsNil() {
        let pool = TrUAPIChainConnectionPool(engineResolver: { _ in nil }, logger: StubLogger())
        #expect(pool.connect(genesisHash: Data(repeating: 0xFF, count: 32)) == nil)
    }

    /// A chain the registry knows but holds no connection for is UNSUPPORTED:
    /// connect returns nil (the core maps None to "chain provider
    /// unavailable") — the pool never dials nodes itself.
    @Test func knownChainWithoutAppConnectionReturnsNil() {
        let pool = TrUAPIChainConnectionPool(
            engineResolver: makeRegistryResolver(chainRegistry: makeChainRegistry()),
            logger: StubLogger()
        )
        #expect(pool.connect(genesisHash: genesis) == nil)
    }

    @Test func registryBackedResolverUsesAppConnection() throws {
        let chainRegistry = makeChainRegistry()
        let connection = MockChainConnection()
        let chainId = try #require(chainRegistry.chainsByGenesis[genesis.toHex()]).chainId
        chainRegistry.connectionsByChainId[chainId] = connection

        let pool = TrUAPIChainConnectionPool(
            engineResolver: makeRegistryResolver(chainRegistry: chainRegistry),
            logger: StubLogger()
        )

        let connectionId = try #require(pool.connect(genesisHash: genesis))
        try pool.send(
            connectionId: connectionId,
            request: #"{"jsonrpc":"2.0","id":1,"method":"m","params":[]}"#
        )
        #expect(connection.calls.count == 1)
    }

    @Test func closeTearsDownOnlyThatConnection() throws {
        let shared = MockJSONRPCEngine()
        let pool = TrUAPIChainConnectionPool(engineResolver: { _ in shared }, logger: StubLogger())

        let first = try #require(pool.connect(genesisHash: genesis))
        let second = try #require(pool.connect(genesisHash: genesis))
        try pool.send(
            connectionId: first,
            request: #"{"jsonrpc":"2.0","id":1,"method":"chain_subscribeNewHeads","params":[]}"#
        )

        pool.close(connectionId: first)

        #expect(!shared.cancelled.isEmpty) // first's subscription unsubscribed
        #expect(throws: (any Error).self) {
            try pool.send(connectionId: first, request: "{}")
        }
        try pool.send(
            connectionId: second,
            request: #"{"jsonrpc":"2.0","id":2,"method":"m","params":[]}"#
        )
    }
}
