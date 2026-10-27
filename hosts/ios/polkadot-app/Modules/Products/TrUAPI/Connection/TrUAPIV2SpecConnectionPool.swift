import Foundation
import os
import ChainRegistry
import Foundation_iOS
import FoundationExt
import SDKLogger

/// One dedicated node socket per core `chain_connect`; socket deaths are reported, never hidden by reconnects.
///
/// Backgrounding alone never closes or probes a socket, so the pool itself gives products no reason to
/// retry against an app about to be suspended; real transport deaths are still reported whenever they
/// happen, including while the app keeps running in the background. On return to the foreground every
/// socket that lived through the background is revalidated: after a long absence all are expired,
/// after a short one each is probed and expired only if silent.
public final class TrUAPIV2SpecConnectionPool: TrUAPIChainConnecting, @unchecked Sendable {
    public typealias HopEndpointResolver = @Sendable (_ bulletinGenesisHash: Data) -> [String]
    private static let maximumHopConnections = 16

    /// Keep a send from reopening a HOP socket after its lease was closed.
    fileprivate final class Connection {
        let socket: ChainSocketProtocol
        let isHop: Bool
        private let lock = NSRecursiveLock()
        private var closed = false
        private var opened = false

        init(socket: ChainSocketProtocol, isHop: Bool = false) {
            self.socket = socket
            self.isHop = isHop
        }

        func open() {
            lock.lock()
            defer { lock.unlock() }
            guard !closed, !opened else { return }
            opened = true
            socket.open()
        }

        func send(connectionId: UInt32, request: String) throws {
            lock.lock()
            defer { lock.unlock() }
            guard !closed else {
                throw TrUAPIChainConnectionError.unknownConnection(connectionId)
            }
            if isHop { open() }
            socket.send(text: request)
        }

        func close() {
            lock.lock()
            defer { lock.unlock() }
            guard !closed else { return }
            closed = true
            socket.close()
        }
    }

    public struct ResumePolicy: Sendable {
        /// Background time after which sockets are expired instead of probed.
        public let expiryThreshold: TimeInterval
        public let probeTimeout: TimeInterval

        public init(expiryThreshold: TimeInterval, probeTimeout: TimeInterval) {
            self.expiryThreshold = expiryThreshold
            self.probeTimeout = probeTimeout
        }

        public static let `default` = ResumePolicy(expiryThreshold: 30, probeTimeout: 5)
    }

    fileprivate struct State {
        weak var eventHandler: TrUAPIChainEventHandling?
        var connections: [UInt32: Connection] = [:]
        var connectionIds: [ObjectIdentifier: UInt32] = [:]
        var nextConnectionId: UInt32 = 1
        var hopConnectionCount = 0
        /// Grows per chain so a reconnect after a dead node lands on the next one.
        var nodeAttempts: [ChainModel.Id: Int] = [:]
        var backgroundedAt: Date?
        /// Tail of the serial chain that keeps background and foreground handling in order.
        var lifecycleTask: Task<Void, Never>?
    }

    public var eventHandler: TrUAPIChainEventHandling? {
        get { state.withLockUnchecked { $0.eventHandler } }
        set { state.withLockUnchecked { $0.eventHandler = newValue } }
    }

    private let chainRegistry: ChainRegistryProtocol
    private let applicationHandler: ApplicationHandlerProtocol
    private let dateProvider: DateProviding
    private let resumePolicy: ResumePolicy
    private let logger: SDKLoggerProtocol
    private let hopEndpointResolver: HopEndpointResolver
    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    public init(
        chainRegistry: ChainRegistryProtocol,
        applicationHandler: ApplicationHandlerProtocol,
        dateProvider: DateProviding,
        resumePolicy: ResumePolicy = .default,
        logger: SDKLoggerProtocol,
        hopEndpointResolver: HopEndpointResolver? = nil
    ) {
        self.chainRegistry = chainRegistry
        self.applicationHandler = applicationHandler
        self.dateProvider = dateProvider
        self.resumePolicy = resumePolicy
        self.logger = logger
        self.hopEndpointResolver = hopEndpointResolver ?? { bulletinGenesisHash in
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
        }

        applicationHandler.delegate = self
    }

    deinit {
        closeAll()
    }

    public func connect(genesisHash: Data) -> UInt32? {
        guard let socket = makeSocket(for: genesisHash) else {
            logger.debug("TrUAPI chain \(genesisHash.toHex()) has no host-managed connection; unsupported")
            return nil
        }

        socket.delegate = self

        let connection = Connection(socket: socket)
        // Registered before open so a close fired during open resolves to this id.
        guard let connectionId = state.withLockUnchecked({ $0.register(connection) }) else { return nil }
        connection.open()

        return connectionId
    }

    public func allowedHopEndpoints(bulletinGenesisHash: Data) -> [String] {
        guard bulletinGenesisHash.count == 32 else { return [] }
        return hopEndpointResolver(bulletinGenesisHash).filter { Self.hopURL(endpoint: $0) != nil }
    }

    public func hopConnect(bulletinGenesisHash: Data, endpoint: String) throws -> UInt32? {
        let endpoints = allowedHopEndpoints(bulletinGenesisHash: bulletinGenesisHash)
        guard !endpoints.isEmpty else { return nil }
        guard
            endpoints.contains(where: { $0.utf8.elementsEqual(endpoint.utf8) }),
            let url = Self.hopURL(endpoint: endpoint)
        else {
            throw TrUAPIChainConnectionError.untrustedHopEndpoint
        }

        return try state.withLockUnchecked { state in
            guard state.hopConnectionCount < Self.maximumHopConnections else {
                throw TrUAPIChainConnectionError.hopConnectionLimitReached
            }
            let dialed = OSAllocatedUnfairLock(initialState: false)
            let transportFactory = ConnectionTransportFactory { [hopEndpointResolver] dialURL in
                dialed.withLock { attempted in
                    guard !attempted else { return false }
                    attempted = true
                    return dialURL.absoluteString.utf8.elementsEqual(endpoint.utf8)
                        && hopEndpointResolver(bulletinGenesisHash).contains {
                            $0.utf8.elementsEqual(endpoint.utf8)
                        }
                }
            }
            // HOP frames and transport failures can contain private tickets.
            let socket = ChainSocket(url: url, transportFactory: transportFactory, logger: nil)
            socket.delegate = self
            // Rust installs its receiver after this callback returns. First send opens
            // the raw socket, which queues the frame until the one allowed dial completes.
            return state.register(Connection(socket: socket, isHop: true))
        }
    }

    public func send(connectionId: UInt32, request: String) throws {
        guard let connection = state.withLockUnchecked({ $0.connections[connectionId] }) else {
            throw TrUAPIChainConnectionError.unknownConnection(connectionId)
        }

        try connection.send(connectionId: connectionId, request: request)
    }

    public func close(connectionId: UInt32) {
        let (connection, handler) = state.withLockUnchecked { state in
            (state.removeConnection(for: connectionId), state.eventHandler)
        }
        connection?.close()
        if connection?.isHop == true {
            handler?.chainDidClose(connectionId: connectionId)
        }
    }

    public func closeAll() {
        let (connections, handler) = state.withLockUnchecked { state in
            let connections = state.connections
            state.connections.removeAll()
            state.connectionIds.removeAll()
            state.hopConnectionCount = 0
            return (connections, state.eventHandler)
        }
        for (id, connection) in connections {
            connection.close()
            if connection.isHop {
                handler?.chainDidClose(connectionId: id)
            }
        }
    }
}

extension TrUAPIV2SpecConnectionPool: ChainSocketDelegate {
    public func chainSocket(_ socket: ChainSocketProtocol, didReceive text: String) {
        let (connectionId, handler) = state.withLockUnchecked { state in
            (state.connectionIds[ObjectIdentifier(socket)], state.eventHandler)
        }

        guard let connectionId else { return }
        handler?.chainDidReceiveResponse(connectionId: connectionId, json: text)
    }

    public func chainSocket(_ socket: ChainSocketProtocol, didCloseWith reason: ChainSocketCloseReason) {
        let (connectionId, connection, handler) = state.withLockUnchecked { state -> (UInt32?, Connection?, TrUAPIChainEventHandling?) in
            guard let connectionId = state.connectionIds[ObjectIdentifier(socket)] else {
                return (nil, nil, nil)
            }
            return (connectionId, state.removeConnection(for: connectionId), state.eventHandler)
        }

        guard let connectionId else { return }

        connection?.close()
        if connection?.isHop != true {
            logger.debug("TrUAPI chain connection \(connectionId) closed: \(reason)")
        }
        handler?.chainDidClose(connectionId: connectionId)
    }
}

extension TrUAPIV2SpecConnectionPool: ApplicationHandlerDelegate {
    public func didReceiveDidEnterBackground(notification _: Notification) {
        enqueueLifecycleEvent { pool in
            await pool.markBackgrounded()
        }
    }

    public func didReceiveWillEnterForeground(notification _: Notification) {
        enqueueLifecycleEvent { pool in
            await pool.revalidateAfterBackground()
        }
    }
}

extension TrUAPIV2SpecConnectionPool {
    /// Waits until every background/foreground notification received so far has been handled.
    func awaitLifecycleEvents() async {
        await state.withLockUnchecked { $0.lifecycleTask }?.value
    }
}

private extension TrUAPIV2SpecConnectionPool {
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

    func makeSocket(for genesisHash: Data) -> ChainSocketProtocol? {
        guard let chain = chainRegistry.getChainByGenesis(for: genesisHash.toHex()) else { return nil }

        let attempt = state.withLockUnchecked { $0.takeNodeAttempt(for: chain.chainId) }
        return chainRegistry.makeChainSocket(for: chain.chainId, attempt: attempt)
    }

    // Date reads are async, so each event runs after the previous one to keep background before foreground.
    func enqueueLifecycleEvent(_ handle: @escaping @Sendable (TrUAPIV2SpecConnectionPool) async -> Void) {
        state.withLockUnchecked { state in
            let previous = state.lifecycleTask
            state.lifecycleTask = Task { [weak self] in
                await previous?.value
                guard let self else { return }
                await handle(self)
            }
        }
    }

    func markBackgrounded() async {
        let enteredAt = await dateProvider.read()

        state.withLockUnchecked { state in
            state.backgroundedAt = state.backgroundedAt ?? enteredAt
        }
    }

    func revalidateAfterBackground() async {
        guard let backgroundedAt = state.withLockUnchecked({ $0.backgroundedAt }) else { return }

        let duration = await dateProvider.read().timeIntervalSince(backgroundedAt)

        let sockets = state.withLockUnchecked { state in
            state.backgroundedAt = nil
            return state.connections.values.map(\.socket)
        }

        revalidate(sockets, afterBackground: duration)
    }

    // Socket callbacks re-enter the pool lock, so sockets are touched only outside it.
    func revalidate(_ sockets: [ChainSocketProtocol], afterBackground duration: TimeInterval) {
        guard !sockets.isEmpty else { return }

        if duration >= resumePolicy.expiryThreshold {
            logger.debug("TrUAPI expiring \(sockets.count) chain sockets after \(Int(duration))s in background")
            sockets.forEach { $0.expire() }
        } else {
            logger.debug("TrUAPI probing \(sockets.count) chain sockets after \(Int(duration))s in background")
            sockets.forEach { $0.probe(timeout: resumePolicy.probeTimeout) }
        }
    }
}

private extension TrUAPIV2SpecConnectionPool.State {
    mutating func register(_ connection: TrUAPIV2SpecConnectionPool.Connection) -> UInt32? {
        // Never reuse an id that a delayed native callback could still carry.
        guard nextConnectionId < UInt32.max else { return nil }
        let id = nextConnectionId
        nextConnectionId += 1
        connections[id] = connection
        connectionIds[ObjectIdentifier(connection.socket)] = id
        if connection.isHop { hopConnectionCount += 1 }
        return id
    }

    mutating func takeNodeAttempt(for chainId: ChainModel.Id) -> Int {
        let attempt = nodeAttempts[chainId, default: 0]
        nodeAttempts[chainId] = attempt + 1
        return attempt
    }

    mutating func removeConnection(for connectionId: UInt32) -> TrUAPIV2SpecConnectionPool.Connection? {
        guard let connection = connections.removeValue(forKey: connectionId) else { return nil }
        connectionIds.removeValue(forKey: ObjectIdentifier(connection.socket))
        if connection.isHop { hopConnectionCount -= 1 }
        return connection
    }
}
