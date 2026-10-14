import Foundation
import SubstrateSdk
import Foundation_iOS

public protocol ConnectionPoolProtocol {
    func setupConnection(for chain: ChainModel) throws -> ChainConnection
    func getConnection(for chainId: ChainModel.Id) -> ChainConnection?
    func retainConnections(_ scope: ConnectionRetainScope) -> ConnectionRetainToken
    func subscribe(_ subscriber: ConnectionStateSubscription, chainId: ChainModel.Id)
    func unsubscribe(_ subscriber: ConnectionStateSubscription, chainId: ChainModel.Id)
    func getOneShotConnection(for chain: ChainModel) -> JSONRPCEngine?
    func deactivateConnection(for chainId: ChainModel.Id)
}

public protocol ConnectionStateSubscription: AnyObject {
    func didReceive(
        state: WebSocketEngine.State,
        for chainId: ChainModel.Id
    )
    func didSwitchURL(
        _ connection: ChainConnection,
        newURL: URL,
        for chainId: ChainModel.Id
    )
}

public extension ConnectionStateSubscription {
    func didSwitchURL(
        _: ChainConnection,
        newURL _: URL,
        for _: ChainModel.Id
    ) {}
}

public class ConnectionPool {
    public let connectionFactory: ConnectionFactoryProtocol
    public let applicationHandler: ApplicationHandlerProtocol

    private var mutex = NSLock()

    private(set) var connections: [ChainModel.Id: WeakWrapper] = [:]
    private(set) var oneShotConnections: [ChainModel.Id: OneShotConnection] = [:]

    private(set) var stateSubscriptions: [ChainModel.Id: [WeakWrapper]] = [:]

    private func clearUnusedConnections() {
        connections = connections.filter { $0.value.target != nil }
    }

    private let retention: ConnectionRetentionState

    public init(
        connectionFactory: ConnectionFactoryProtocol,
        applicationHandler: ApplicationHandlerProtocol
    ) {
        self.connectionFactory = connectionFactory
        self.applicationHandler = applicationHandler
        retention = ConnectionRetentionState()

        applicationHandler.delegate = self
    }
}

extension ConnectionPool: ConnectionPoolProtocol {
    public func subscribe(_ subscriber: ConnectionStateSubscription, chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let subscribers = stateSubscriptions[chainId], subscribers.contains(where: { $0.target === subscriber }) {
            return
        }

        var subscribers = stateSubscriptions[chainId] ?? []
        subscribers.append(WeakWrapper(target: subscriber))
        stateSubscriptions[chainId] = subscribers

        let connection = connections[chainId]?.target as? ChainConnection

        DispatchQueue.main.async {
            subscriber.didReceive(state: connection?.state ?? .notConnected(url: nil), for: chainId)
        }
    }

    public func unsubscribe(_ subscriber: ConnectionStateSubscription, chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let subscribers = stateSubscriptions[chainId]
        stateSubscriptions[chainId] = subscribers?.filter { $0.target !== subscriber }
    }

    public func setupConnection(for chain: ChainModel) throws -> ChainConnection {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        clearUnusedConnections()

        if let connection = connections[chain.chainId]?.target as? ChainConnection {
            connectionFactory.updateConnection(connection, chain: chain)

            if case .notConnected = connection.state {
                connection.connect()
            }

            return connection
        }

        let connection = try connectionFactory.createConnection(for: chain, delegate: self)
        connections[chain.chainId] = WeakWrapper(target: connection)

        return connection
    }

    public func deactivateConnection(for chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let optConnection = connections[chainId]?.target
        oneShotConnections[chainId] = nil

        clearUnusedConnections()

        if let connection = optConnection as? ChainConnection {
            connection.disconnect(true)
        }
    }

    public func getConnection(for chainId: ChainModel.Id) -> ChainConnection? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return connections[chainId]?.target as? ChainConnection
    }

    public func getOneShotConnection(for chain: ChainModel) -> JSONRPCEngine? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        if let existingConnection = oneShotConnections[chain.chainId] {
            connectionFactory.updateOneShotConnection(existingConnection, chain: chain)

            return existingConnection
        }

        if let connection = try? connectionFactory.createOneShotConnection(for: chain) {
            oneShotConnections[chain.chainId] = connection

            return connection
        } else {
            return connections[chain.chainId]?.target as? JSONRPCEngine
        }
    }

    public func retainConnections(_ scope: ConnectionRetainScope) -> ConnectionRetainToken {
        mutex.lock()

        defer { mutex.unlock() }

        let resolved: ConnectionRetentionState.ResolvedScope =
            switch scope {
            case let .chains(chainIds):
                .connections(chainIds.compactMap { connections[$0]?.target as? ChainConnection })
            case .all:
                .all(current: liveConnectionsLocked())
            }

        let (token, effect) = retention.retain(resolved, owner: self)
        apply(effect)

        return token
    }
}

extension ConnectionPool: ConnectionRetaining {
    func releaseRetain(_ retained: ConnectionRetainToken.Retained) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let effect = retention.release(
            retained,
            currentConnections: liveConnectionsLocked()
        )
        apply(effect)
    }
}

private extension ConnectionPool {
    func liveConnectionsLocked() -> [ChainConnection] {
        connections.values.compactMap { $0.target as? ChainConnection }
    }

    func apply(_ effect: ConnectionRetentionState.Effect) {
        effect.wake.forEach { $0.connect() }
        effect.sleep.forEach { $0.disconnect(true) }
    }
}

extension ConnectionPool: WebSocketEngineDelegate {
    public func webSocketDidSwitchURL(
        _ connection: AnyObject,
        newUrl: URL
    ) {
        processWebsocketChange(
            connection: connection,
            newState: nil,
            newUrl: newUrl
        )
    }

    public func webSocketDidChangeState(
        _ connection: AnyObject,
        from _: WebSocketEngine.State,
        to newState: WebSocketEngine.State
    ) {
        processWebsocketChange(
            connection: connection,
            newState: newState,
            newUrl: nil
        )
    }

    public func processWebsocketChange(
        connection: AnyObject,
        newState: WebSocketEngine.State?,
        newUrl: URL?
    ) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let allChainIds = connections.keys
        let maybeChainId = allChainIds.first(where: { connections[$0]?.target === connection })

        guard
            let chainId = maybeChainId,
            let chainConnection = connection as? ChainConnection
        else {
            return
        }

        let maybeSubscriptions = stateSubscriptions[chainId]?.compactMap { $0.target as? ConnectionStateSubscription }

        guard let subscriptions = maybeSubscriptions, !subscriptions.isEmpty else {
            return
        }

        DispatchQueue.main.async {
            if let newUrl {
                subscriptions.forEach {
                    $0.didSwitchURL(
                        chainConnection,
                        newURL: newUrl,
                        for: chainId
                    )
                }
            } else if let newState {
                subscriptions.forEach {
                    $0.didReceive(
                        state: newState,
                        for: chainId
                    )
                }
            }
        }
    }
}

extension ConnectionPool: ApplicationHandlerDelegate {
    public func didReceiveDidBecomeActive(notification _: Notification) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        retention.becomeActive()

        liveConnectionsLocked().forEach { $0.connect() }
    }

    public func didReceiveDidEnterBackground(notification _: Notification) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        let effect = retention.enterBackground(connections: liveConnectionsLocked())
        apply(effect)
    }
}
