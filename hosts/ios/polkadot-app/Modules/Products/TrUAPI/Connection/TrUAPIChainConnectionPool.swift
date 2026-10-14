import Foundation
import os
import SDKLogger
import SubstrateSdk

public protocol TrUAPIChainConnecting: AnyObject, Sendable {
    /// Receives responses/termination events for all logical connections.
    var eventHandler: TrUAPIChainEventHandling? { get set }

    func connect(genesisHash: Data) -> UInt32?
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
}

/// JSON-RPC pipe per logical chain connection, multiplexed over the host's
/// shared per-chain engines: each connect wraps a TrUAPIChainRpcAdapter
/// around the engine the resolver returns, so N sessions share one physical
/// socket per chain. A chain the resolver cannot serve is unsupported
/// (connect returns nil → the core maps None to "chain provider
/// unavailable"). `chainDidClose` is never emitted here: the shared engine
/// reconnects transparently and stateful subscriptions surface reconnects as
/// synthesized termination events, which is the core's designed recovery path.
public final class TrUAPIChainConnectionPool: TrUAPIChainConnecting, @unchecked Sendable {
    /// Resolves the shared engine for a chain genesis hash. The host injects
    /// the lookup (e.g. via its chain registry); the pool never dials nodes.
    public typealias EngineResolver = @Sendable (_ genesisHash: Data) -> JSONRPCEngine?

    private struct State {
        weak var eventHandler: TrUAPIChainEventHandling?
        var adapters: [UInt32: TrUAPIChainRpcAdapter] = [:]
        var connectionIds: [ObjectIdentifier: UInt32] = [:]
        var nextConnectionId: UInt32 = 1
    }

    public var eventHandler: TrUAPIChainEventHandling? {
        get { state.withLock { $0.eventHandler } }
        set { state.withLock { $0.eventHandler = newValue } }
    }

    private let engineResolver: EngineResolver
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock<State>(initialState: State())

    public init(engineResolver: @escaping EngineResolver, logger: SDKLoggerProtocol) {
        self.engineResolver = engineResolver
        self.logger = logger
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
            let connectionId = state.nextConnectionId
            state.nextConnectionId += 1
            state.adapters[connectionId] = adapter
            state.connectionIds[ObjectIdentifier(adapter)] = connectionId
            return connectionId
        }
    }

    public func send(connectionId: UInt32, request: String) throws {
        guard let adapter = adapter(for: connectionId) else {
            throw TrUAPIChainConnectionError.unknownConnection(connectionId)
        }
        adapter.handle(request: request)
    }

    public func close(connectionId: UInt32) {
        let adapter = state.withLock { state -> TrUAPIChainRpcAdapter? in
            let adapter = state.adapters.removeValue(forKey: connectionId)
            if let adapter {
                state.connectionIds.removeValue(forKey: ObjectIdentifier(adapter))
            }
            return adapter
        }
        adapter?.tearDown()
    }

    public func closeAll() {
        let all = state.withLock { state -> [UInt32: TrUAPIChainRpcAdapter] in
            let all = state.adapters
            state.adapters.removeAll()
            state.connectionIds.removeAll()
            return all
        }
        all.values.forEach { $0.tearDown() }
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

private extension TrUAPIChainConnectionPool {
    func adapter(for connectionId: UInt32) -> TrUAPIChainRpcAdapter? {
        state.withLock { $0.adapters[connectionId] }
    }
}
