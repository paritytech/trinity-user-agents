import Foundation
import os
import SDKLogger
import SubstrateSdk
import ChainRegistry

public protocol TrUAPIChainConnecting: AnyObject, Sendable {
    /// Receives responses/termination events for all logical connections.
    var eventHandler: TrUAPIChainEventHandling? { get set }

    func connect(genesisHash: Data) -> UInt32?
    func allowedHopEndpoints(bulletinGenesisHash: Data) -> [String]
    func hopConnect(bulletinGenesisHash: Data, endpoint: String) throws -> UInt32?
    func send(connectionId: UInt32, request: String) throws
    func close(connectionId: UInt32)
    func closeAll()
}

public protocol TrUAPIChainEventHandling: AnyObject {
    func chainDidReceiveResponse(connectionId: UInt32, json: String)
    func chainDidClose(connectionId: UInt32)
}

public enum TrUAPIChainConnectionError: Error {
    case unknownConnection(UInt32)
    case untrustedHopEndpoint
    case hopConnectionLimitReached
}

/// Shared chain engines and pool-owned HOP engines use the same logical
/// connection ids and JSON-RPC adapter. Shared chains retain their transparent
/// reconnection behavior; a HOP connection ends when its dedicated socket
/// disconnects, so the core must recheck live trust before opening another.
public final class TrUAPIChainConnectionPool: TrUAPIChainConnecting, @unchecked Sendable {
    public typealias EngineResolver = @Sendable (_ genesisHash: Data) -> JSONRPCEngine?
    public typealias HopEndpointResolver = @Sendable (_ bulletinGenesisHash: Data) -> [String]

    private static let maximumHopConnections = 16

    /// Serializes sends against close so a racing sender cannot restart
    /// an engine after it has been removed from the pool.
    private final class Connection {
        let adapter: TrUAPIChainRpcAdapter
        let ownedEngine: WebSocketEngine?
        private let lock = NSRecursiveLock()
        private var closed = false

        init(adapter: TrUAPIChainRpcAdapter, ownedEngine: WebSocketEngine? = nil) {
            self.adapter = adapter
            self.ownedEngine = ownedEngine
        }

        func send(connectionId: UInt32, request: String) throws {
            lock.lock()
            defer { lock.unlock() }
            guard !closed else {
                throw TrUAPIChainConnectionError.unknownConnection(connectionId)
            }
            adapter.handle(request: request)
        }

        func close() {
            lock.lock()
            defer { lock.unlock() }
            guard !closed else { return }
            closed = true
            ownedEngine?.delegate = nil
            adapter.tearDown()
            ownedEngine?.disconnectIfNeeded(true)
        }
    }

    private struct State {
        weak var eventHandler: TrUAPIChainEventHandling?
        var connections: [UInt32: Connection] = [:]
        var connectionIds: [ObjectIdentifier: UInt32] = [:]
        var ownedEngineIds: [ObjectIdentifier: UInt32] = [:]
        var nextConnectionId: UInt32 = 1
    }

    public var eventHandler: TrUAPIChainEventHandling? {
        get { state.withLock { $0.eventHandler } }
        set { state.withLock { $0.eventHandler = newValue } }
    }

    private let engineResolver: EngineResolver
    private let hopEndpointResolver: HopEndpointResolver
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    public init(
        engineResolver: @escaping EngineResolver,
        hopEndpointResolver: @escaping HopEndpointResolver = { _ in [] },
        logger: SDKLoggerProtocol
    ) {
        self.engineResolver = engineResolver
        self.hopEndpointResolver = hopEndpointResolver
        self.logger = logger
    }

    /// Resolve the configured Bulletin chain on every lookup, never a cached
    /// endpoint snapshot or the chain named by an untrusted endpoint.
    convenience init(chainRegistry: ChainRegistryProtocol, logger: SDKLoggerProtocol) {
        self.init(
            engineResolver: { genesisHash in
                chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap { chain in
                    chainRegistry.getConnection(for: chain.chainId)
                }
            },
            hopEndpointResolver: { bulletinGenesisHash in
                guard
                    let chain = chainRegistry.getChain(for: AppConfig.Chains.bulletInChain),
                    let genesisHex = chain.explicitGenesisHash,
                    let genesisHash = try? Data(hexString: genesisHex),
                    genesisHash == bulletinGenesisHash,
                    let apis = chain.externalApis?.hop()
                else {
                    return []
                }
                return apis.map { $0.url.absoluteString }.sorted()
            },
            logger: logger
        )
    }

    deinit {
        closeAll()
    }

    public func connect(genesisHash: Data) -> UInt32? {
        guard let engine = engineResolver(genesisHash) else {
            logger.debug(
                "TrUAPI chain \(genesisHash.toHex()) has no host-managed connection; unsupported"
            )
            return nil
        }

        let adapter = TrUAPIChainRpcAdapter(engine: engine, logger: logger)
        adapter.delegate = self

        return state.withLock { state in
            register(Connection(adapter: adapter), state: &state)
        }
    }

    public func allowedHopEndpoints(bulletinGenesisHash: Data) -> [String] {
        guard bulletinGenesisHash.count == 32 else { return [] }
        return hopEndpointResolver(bulletinGenesisHash).filter { Self.hopURL(endpoint: $0) != nil }
    }

    public func hopConnect(bulletinGenesisHash: Data, endpoint: String) throws -> UInt32? {
        let endpoints = allowedHopEndpoints(bulletinGenesisHash: bulletinGenesisHash)
        guard !endpoints.isEmpty else { return nil }
        // Byte-exact comparison, not URL equality, origin matching, or Swift's
        // Unicode-normalizing String equality.
        guard
            endpoints.contains(where: { $0.utf8.elementsEqual(endpoint.utf8) }),
            let url = Self.hopURL(endpoint: endpoint)
        else {
            throw TrUAPIChainConnectionError.untrustedHopEndpoint
        }

        // Rust installs its response/close receiver after this callback
        // returns. Keep the engine idle until the first send: SDK callMethod
        // queues that request and starts the socket itself.
        return try state.withLock { state -> UInt32? in
            guard state.ownedEngineIds.count < Self.maximumHopConnections else {
                throw TrUAPIChainConnectionError.hopConnectionLimitReached
            }
            let dialed = OSAllocatedUnfairLock(initialState: false)
            let transportFactory = ConnectionTransportFactory { [hopEndpointResolver] dialURL in
                dialed.withLock { attempted in
                    guard !attempted else { return false }
                    attempted = true
                    // One physical dial per HOP lease, checked again at the
                    // transport boundary. Core operations own all retries.
                    return dialURL.absoluteString.utf8.elementsEqual(endpoint.utf8)
                        && hopEndpointResolver(bulletinGenesisHash).contains {
                            $0.utf8.elementsEqual(endpoint.utf8)
                        }
                }
            }
            guard let engine = WebSocketEngine(
                urls: [url],
                connectionFactory: transportFactory,
                reconnectionStrategy: nil,
                autoconnect: false,
                logger: nil
            ) else {
                return nil
            }
            // HOP parameters/results contain private tickets and payloads.
            // Neither the SDK engine nor its adapter may log their frames.
            let adapter = TrUAPIChainRpcAdapter(engine: engine, logger: nil, resendOnReconnect: false)
            adapter.delegate = self
            engine.delegate = self
            let connection = Connection(adapter: adapter, ownedEngine: engine)
            guard let connectionId = register(connection, state: &state) else { return nil }
            state.ownedEngineIds[ObjectIdentifier(engine)] = connectionId
            return connectionId
        }
    }

    public func send(connectionId: UInt32, request: String) throws {
        guard let connection = state.withLock({ $0.connections[connectionId] }) else {
            throw TrUAPIChainConnectionError.unknownConnection(connectionId)
        }
        try connection.send(connectionId: connectionId, request: request)
    }

    public func close(connectionId: UInt32) {
        let (connection, handler) = state.withLock { state in
            (remove(connectionId: connectionId, state: &state), state.eventHandler)
        }
        connection?.close()
        if connection?.ownedEngine != nil {
            handler?.chainDidClose(connectionId: connectionId)
        }
    }

    public func closeAll() {
        let (all, handler) = state.withLock { state in
            let all = state.connections
            state.connections.removeAll()
            state.connectionIds.removeAll()
            state.ownedEngineIds.removeAll()
            return (all, state.eventHandler)
        }
        for (connectionId, connection) in all {
            connection.close()
            if connection.ownedEngine != nil {
                handler?.chainDidClose(connectionId: connectionId)
            }
        }
    }
}

extension TrUAPIChainConnectionPool: TrUAPIChainRpcAdapterDelegate {
    public func adapter(_ adapter: TrUAPIChainRpcAdapter, didProduce json: String) {
        let (connectionId, handler) = state.withLock { state in
            (state.connectionIds[ObjectIdentifier(adapter)], state.eventHandler)
        }

        guard let connectionId else { return }
        handler?.chainDidReceiveResponse(connectionId: connectionId, json: json)
    }
}

extension TrUAPIChainConnectionPool: WebSocketEngineDelegate {
    public func webSocketDidChangeState(
        _ connection: AnyObject,
        from oldState: WebSocketEngine.State,
        to newState: WebSocketEngine.State
    ) {
        switch newState {
        case .connected:
            return
        case .connecting:
            // The initial dial is not a close. An established socket being
            // restarted by the engine, however, must not silently replay HOP.
            guard case .connected = oldState else { return }
        case .notConnected, .waitingReconnection:
            break
        }
        closeOwnedEngine(connection)
    }

    public func webSocketDidSwitchURL(_ connection: AnyObject, newUrl _: URL) {
        // Dedicated HOP engines have exactly one allowed URL.
        closeOwnedEngine(connection)
    }
}

private extension TrUAPIChainConnectionPool {
    private struct ClosedConnection {
        let id: UInt32
        let connection: Connection
        let handler: TrUAPIChainEventHandling?
    }

    static func hopURL(endpoint: String) -> URL? {
        let authority = endpoint.dropFirst(6).prefix {
            $0 != "/" && $0 != "?" && $0 != "#"
        }
        guard
            endpoint.hasPrefix("wss://"),
            !endpoint.unicodeScalars.contains(where: {
                $0 == "\\" || $0.properties.isWhitespace || $0.properties.generalCategory == .control
            }),
            !authority.contains("@"),
            let url = URL(string: endpoint),
            url.scheme == "wss",
            url.user == nil,
            url.password == nil,
            url.fragment == nil,
            let host = url.host,
            !host.isEmpty,
            url.absoluteString.utf8.elementsEqual(endpoint.utf8)
        else {
            return nil
        }
        return url
    }

    private func register(_ connection: Connection, state: inout State) -> UInt32? {
        // Never wrap and alias a still-live or delayed native notification.
        guard state.nextConnectionId < UInt32.max else { return nil }
        let connectionId = state.nextConnectionId
        state.nextConnectionId += 1
        state.connections[connectionId] = connection
        state.connectionIds[ObjectIdentifier(connection.adapter)] = connectionId
        return connectionId
    }

    private func remove(connectionId: UInt32, state: inout State) -> Connection? {
        guard let connection = state.connections.removeValue(forKey: connectionId) else { return nil }
        state.connectionIds.removeValue(forKey: ObjectIdentifier(connection.adapter))
        if let engine = connection.ownedEngine {
            state.ownedEngineIds.removeValue(forKey: ObjectIdentifier(engine))
        }
        return connection
    }

    func closeOwnedEngine(_ engine: AnyObject) {
        let closed = state.withLock { state -> ClosedConnection? in
            guard
                let connectionId = state.ownedEngineIds[ObjectIdentifier(engine)],
                let connection = remove(connectionId: connectionId, state: &state)
            else {
                return nil
            }
            return ClosedConnection(
                id: connectionId,
                connection: connection,
                handler: state.eventHandler
            )
        }
        guard let closed else { return }
        closed.connection.close()
        closed.handler?.chainDidClose(connectionId: closed.id)
    }
}
