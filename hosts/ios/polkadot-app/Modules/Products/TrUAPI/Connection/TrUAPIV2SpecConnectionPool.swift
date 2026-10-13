import Foundation
import os
import ChainRegistry
import SDKLogger

/// One dedicated node socket per core `chain_connect`; socket deaths are reported, never hidden by reconnects.
public final class TrUAPIV2SpecConnectionPool: TrUAPIChainConnecting, @unchecked Sendable {
    public typealias SocketResolver = @Sendable (_ genesisHash: Data) -> ChainSocketProtocol?

    fileprivate struct State {
        weak var eventHandler: TrUAPIChainEventHandling?
        var sockets: [UInt32: ChainSocketProtocol] = [:]
        var connectionIds: [ObjectIdentifier: UInt32] = [:]
        var nextConnectionId: UInt32 = 1
    }

    public var eventHandler: TrUAPIChainEventHandling? {
        get { state.withLockUnchecked { $0.eventHandler } }
        set { state.withLockUnchecked { $0.eventHandler = newValue } }
    }

    private let socketResolver: SocketResolver
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock(uncheckedState: State())

    public init(socketResolver: @escaping SocketResolver, logger: SDKLoggerProtocol) {
        self.socketResolver = socketResolver
        self.logger = logger
    }

    public static func makeChainConnections(
        chainRegistry: ChainRegistryProtocol,
        logger: SDKLoggerProtocol
    ) -> TrUAPIV2SpecConnectionPool {
        TrUAPIV2SpecConnectionPool(
            socketResolver: { [chainRegistry] genesisHash in
                chainRegistry.getChainByGenesis(for: genesisHash.toHex()).flatMap {
                    chainRegistry.makeChainSocket(for: $0.chainId)
                }
            },
            logger: logger
        )
    }

    public func connect(genesisHash: Data) -> UInt32? {
        guard let socket = socketResolver(genesisHash) else {
            logger.debug("TrUAPI chain \(genesisHash.toHex()) has no host-managed connection; unsupported")
            return nil
        }

        socket.delegate = self

        // Registered before open so a close fired during open resolves to this id.
        let connectionId = state.withLockUnchecked { state in
            let connectionId = state.nextConnectionId
            state.nextConnectionId += 1
            state.sockets[connectionId] = socket
            state.connectionIds[ObjectIdentifier(socket)] = connectionId
            return connectionId
        }

        socket.open()

        return connectionId
    }

    public func send(connectionId: UInt32, request: String) throws {
        guard let socket = state.withLockUnchecked({ $0.sockets[connectionId] }) else {
            throw TrUAPIChainConnectionError.unknownConnection(connectionId)
        }

        socket.send(text: request)
    }

    public func close(connectionId: UInt32) {
        let socket = state.withLockUnchecked { $0.removeSocket(for: connectionId) }
        socket?.close()
    }

    public func closeAll() {
        let sockets = state.withLockUnchecked { state -> [ChainSocketProtocol] in
            let sockets = Array(state.sockets.values)
            state.sockets.removeAll()
            state.connectionIds.removeAll()
            return sockets
        }

        sockets.forEach { $0.close() }
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
        let (connectionId, handler) = state.withLockUnchecked { state -> (UInt32?, TrUAPIChainEventHandling?) in
            guard let connectionId = state.connectionIds[ObjectIdentifier(socket)] else {
                return (nil, nil)
            }

            _ = state.removeSocket(for: connectionId)
            return (connectionId, state.eventHandler)
        }

        guard let connectionId else { return }

        logger.debug("TrUAPI chain connection \(connectionId) closed: \(reason)")
        handler?.chainDidClose(connectionId: connectionId)
    }
}

private extension TrUAPIV2SpecConnectionPool.State {
    mutating func removeSocket(for connectionId: UInt32) -> ChainSocketProtocol? {
        guard let socket = sockets.removeValue(forKey: connectionId) else { return nil }
        connectionIds.removeValue(forKey: ObjectIdentifier(socket))
        return socket
    }
}
