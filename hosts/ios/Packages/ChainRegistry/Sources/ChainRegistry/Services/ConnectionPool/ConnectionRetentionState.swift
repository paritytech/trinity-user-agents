import Foundation
import SubstrateSdk
import Foundation_iOS

final class ConnectionRetentionState {
    enum ResolvedScope {
        case connections([ChainConnection])
        case all(current: [ChainConnection])
    }

    struct Effect {
        var wake: [ChainConnection] = []
        var sleep: [ChainConnection] = []
    }

    private var retainCounts: [ObjectIdentifier: Int] = [:]
    private var allRetainCount = 0
    private var isSleeping = false

    func retain(
        _ scope: ResolvedScope,
        owner: ConnectionRetaining
    ) -> (token: ConnectionRetainToken, effect: Effect) {
        switch scope {
        case let .connections(connections):
            retainEach(connections, owner: owner)
        case let .all(current):
            retainAll(current: current, owner: owner)
        }
    }

    func release(
        _ retained: ConnectionRetainToken.Retained,
        currentConnections: [ChainConnection]
    ) -> Effect {
        switch retained {
        case let .connections(list):
            releaseEach(list)
        case .all:
            releaseAll(currentConnections: currentConnections)
        }
    }

    func enterBackground(connections: [ChainConnection]) -> Effect {
        isSleeping = true

        guard allRetainCount == 0 else {
            return Effect()
        }

        return Effect(sleep: unretained(connections))
    }

    func becomeActive() {
        isSleeping = false
    }
}

// MARK: - Private

private extension ConnectionRetentionState {
    func retainEach(
        _ connections: [ChainConnection],
        owner: ConnectionRetaining
    ) -> (token: ConnectionRetainToken, effect: Effect) {
        var retained: [(id: ObjectIdentifier, connection: AnyObject)] = []
        var effect = Effect()

        connections.forEach { connection in
            retained.append((ObjectIdentifier(connection), connection))
            guard bump(connection) else { return }
            effect.wake.append(connection)
        }

        let token = ConnectionRetainToken(
            owner: owner,
            retained: .connections(retained)
        )

        return (token, effect)
    }

    func retainAll(
        current: [ChainConnection],
        owner: ConnectionRetaining
    ) -> (token: ConnectionRetainToken, effect: Effect) {
        allRetainCount += 1

        let effect = allRetainCount == 1 && isSleeping
            ? Effect(wake: current)
            : Effect()

        let token = ConnectionRetainToken(
            owner: owner,
            retained: .all(snapshot: current)
        )

        return (token, effect)
    }

    func bump(_ connection: ChainConnection) -> Bool {
        let id = ObjectIdentifier(connection)
        let newCount = (retainCounts[id] ?? 0) + 1
        retainCounts[id] = newCount

        return newCount == 1 && isSleeping
    }

    func releaseEach(_ list: [(id: ObjectIdentifier, connection: AnyObject)]) -> Effect {
        list.reduce(into: Effect()) { effect, item in
            guard let connection = releaseSingle(
                id: item.id,
                connection: item.connection
            ) else { return }

            effect.sleep.append(connection)
        }
    }

    func releaseSingle(
        id: ObjectIdentifier,
        connection: AnyObject?
    ) -> ChainConnection? {
        guard let count = retainCounts[id] else {
            return nil
        }

        guard count <= 1 else {
            retainCounts[id] = count - 1
            return nil
        }

        retainCounts[id] = nil

        guard isSleeping, let connection = connection as? ChainConnection else {
            return nil
        }

        return connection
    }

    func releaseAll(currentConnections: [ChainConnection]) -> Effect {
        guard allRetainCount > 0 else {
            return Effect()
        }

        allRetainCount -= 1

        guard allRetainCount == 0, isSleeping else {
            return Effect()
        }

        return Effect(sleep: unretained(currentConnections))
    }

    func unretained(_ connections: [ChainConnection]) -> [ChainConnection] {
        connections.filter { retainCounts[ObjectIdentifier($0)] == nil }
    }
}
