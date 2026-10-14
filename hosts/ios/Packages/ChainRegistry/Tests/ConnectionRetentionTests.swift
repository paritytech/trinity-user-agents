import Testing
import Foundation
import SubstrateSdk
import Foundation_iOS
import ChainRegistry

struct ConnectionRetentionTests {
    // MARK: - .chains

    @Test func retainKeepsConnectionAwakeAcrossBackground() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")

        let token = pool.retainConnections(.chains(["chain-1"]))
        pool.enterBackground()

        withExtendedLifetime(token) {
            #expect(connection.disconnectCallCount == 0)
        }
    }

    @Test func releaseWhileBackgroundedDisconnects() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")

        var token: ConnectionRetainToken? = pool.retainConnections(.chains(["chain-1"]))
        pool.enterBackground()
        #expect(connection.disconnectCallCount == 0)

        token = nil
        _ = token

        #expect(connection.disconnectCallCount == 1)
    }

    @Test func retainWhileBackgroundedReconnects() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")
        pool.enterBackground()
        #expect(connection.disconnectCallCount == 1)

        let token = pool.retainConnections(.chains(["chain-1"]))

        withExtendedLifetime(token) {
            #expect(connection.connectCallCount == 1)
        }
    }

    @Test func overlappingHoldersKeepConnectionUntilLastRelease() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")

        var first: ConnectionRetainToken? = pool.retainConnections(.chains(["chain-1"]))
        var second: ConnectionRetainToken? = pool.retainConnections(.chains(["chain-1"]))
        pool.enterBackground()

        #expect(connection.disconnectCallCount == 0)

        first = nil
        _ = first
        #expect(connection.disconnectCallCount == 0)

        second = nil
        _ = second
        #expect(connection.disconnectCallCount == 1)
    }

    @Test func tokenDeinitReleases() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")
        pool.enterBackground()

        do {
            let token = pool.retainConnections(.chains(["chain-1"]))
            withExtendedLifetime(token) {
                #expect(connection.connectCallCount == 1)
            }
        }

        #expect(connection.disconnectCallCount == 2)
    }

    @Test func doubleReleaseIsIdempotent() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")

        let token = pool.retainConnections(.chains(["chain-1"]))
        pool.enterBackground()
        token.release()
        token.release()

        #expect(connection.disconnectCallCount == 1)
    }

    @Test func retainingUnknownChainIsNoOp() {
        let pool = makePool()

        let token = pool.retainConnections(.chains(["missing"]))

        withExtendedLifetime(token) {
            #expect(Bool(true))
        }
    }

    // MARK: - .all

    @Test func retainAllReconnectsAndResleepsEveryConnection() throws {
        let pool = makePool()
        let first = try register(in: pool, id: "chain-1")
        let second = try register(in: pool, id: "chain-2")
        pool.enterBackground()
        #expect(first.disconnectCallCount == 1)
        #expect(second.disconnectCallCount == 1)

        let token = pool.retainConnections(.all)
        #expect(first.connectCallCount == 1)
        #expect(second.connectCallCount == 1)

        token.release()
        #expect(first.disconnectCallCount == 2)
        #expect(second.disconnectCallCount == 2)
    }

    @Test func retainAllKeepsConnectionCreatedAfterRetainAwake() throws {
        let pool = makePool()

        let token = pool.retainConnections(.all)
        let lateComer = try register(in: pool, id: "late")

        pool.enterBackground()
        #expect(lateComer.disconnectCallCount == 0)

        token.release()
        #expect(lateComer.disconnectCallCount == 1)
    }

    @Test func perChainRetainSurvivesAllRelease() throws {
        let pool = makePool()
        let connection = try register(in: pool, id: "chain-1")
        pool.enterBackground()

        let allToken = pool.retainConnections(.all)
        let chainToken = pool.retainConnections(.chains(["chain-1"]))

        allToken.release()
        // Still retained by the per-chain token.
        #expect(connection.disconnectCallCount == 1)

        chainToken.release()
        #expect(connection.disconnectCallCount == 2)
    }
}

// MARK: - Helpers

private extension ConnectionRetentionTests {
    func makePool() -> ConnectionPool {
        ConnectionPool(
            connectionFactory: StubConnectionFactory(),
            applicationHandler: StubApplicationHandler()
        )
    }

    func register(in pool: ConnectionPool, id: ChainModel.Id) throws -> RecordingChainConnection {
        let connection = try pool.setupConnection(for: makeChain(id: id))
        return try #require(connection as? RecordingChainConnection)
    }

    func makeChain(id: ChainModel.Id) -> ChainModel {
        ChainModel(
            chainId: id,
            parentId: nil,
            name: "Test",
            assets: [],
            nodes: [],
            nodeSwitchStrategy: .roundRobin,
            addressPrefix: 0,
            explicitGenesisHash: nil,
            types: nil,
            icon: nil,
            options: nil,
            externalApis: nil,
            explorers: nil,
            order: 0,
            additional: nil,
            syncMode: .full
        )
    }
}

private extension ConnectionPool {
    func enterBackground() {
        didReceiveDidEnterBackground(notification: Notification(name: .init("test.background")))
    }
}

private final class RecordingChainConnection: ChainConnection {
    private(set) var connectCallCount = 0
    private(set) var disconnectCallCount = 0

    func connect() { connectCallCount += 1 }
    func disconnect(_: Bool) { disconnectCallCount += 1 }

    var state: WebSocketEngine.State { .notConnected(url: nil) }

    var urls: [URL] { [] }
    func changeUrls(_: [URL]) {}

    func callMethod<T: Decodable>(
        _: String,
        params _: (some Encodable)?,
        options _: JSONRPCOptions,
        completion _: ((Result<T, Error>) -> Void)?
    ) throws -> UInt16 { 0 }

    func subscribe<T: Decodable>(
        _: String,
        params _: (some Encodable)?,
        unsubscribeMethod _: String,
        options _: JSONRPCOptions,
        onSubscribed _: ((JSONRPCSubscriptionId) -> Void)?,
        updateClosure _: @escaping (T) -> Void,
        failureClosure _: @escaping (Error, Bool) -> Void
    ) throws -> UInt16 { 0 }

    func cancelForIdentifiers(_: [UInt16], sendUnsubscribe _: Bool) {}

    func addBatchCallMethod(_: String, params _: (some Encodable)?, batchId _: JSONRPCBatchId) throws {}

    func submitBatch(
        for _: JSONRPCBatchId,
        options _: JSONRPCOptions,
        completion _: (([Result<JSON, Error>]) -> Void)?
    ) throws -> [UInt16] { [] }

    func clearBatch(for _: JSONRPCBatchId) {}
}

private final class StubConnectionFactory: ConnectionFactoryProtocol {
    func createConnection(
        for _: ChainNodeConnectable,
        delegate _: WebSocketEngineDelegate?
    ) throws -> ChainConnection {
        RecordingChainConnection()
    }

    func createConnection(
        for _: ChainNodeModel,
        chain _: ChainNodeConnectable,
        delegate _: WebSocketEngineDelegate?
    ) throws -> ChainConnection {
        RecordingChainConnection()
    }

    func updateConnection(_: ChainConnection, chain _: ChainNodeConnectable) {}
    func updateOneShotConnection(_: OneShotConnection, chain _: ChainNodeConnectable) {}

    func createOneShotConnection(for _: ChainNodeConnectable) throws -> OneShotConnection {
        throw NSError(domain: "ConnectionRetentionTests", code: 0)
    }
}

private final class StubApplicationHandler: ApplicationHandlerProtocol {
    weak var delegate: ApplicationHandlerDelegate?
}
