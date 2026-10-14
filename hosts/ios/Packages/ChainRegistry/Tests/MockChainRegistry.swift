import Foundation
import os
import Operation_iOS
import SubstrateSdk
import ChainRegistry

/// Subscriber closures are copied out under the lock and invoked after releasing it: calling them
/// while holding the lock reenters `chainsUnsubscribe` and deadlocks the test process.
final class MockChainRegistry: ChainRegistryProtocol {
    private struct SubscriptionRecord {
        weak var target: AnyObject?
        let closure: ([DataProviderChange<ChainModel>]) -> Void
    }

    private struct State {
        var subscriptions: [SubscriptionRecord] = []
        var unsubscribeCount = 0
    }

    private let lock = OSAllocatedUnfairLock(initialState: State())

    var hasSubscription: Bool {
        lock.withLock { !$0.subscriptions.isEmpty }
    }

    var unsubscribeCount: Int {
        lock.withLock { $0.unsubscribeCount }
    }

    var availableChainIds: Set<ChainModel.Id>? {
        nil
    }

    var allAvailableChains: [ChainModel] {
        []
    }

    func getChain(for _: ChainModel.Id) -> ChainModel? {
        nil
    }

    func getChainByGenesis(for _: ChainModel.Id) -> ChainModel? {
        nil
    }

    func getConnection(for _: ChainModel.Id) -> ChainConnection? {
        nil
    }

    func getOneShotConnection(for _: ChainModel.Id) -> JSONRPCEngine? {
        nil
    }

    func getRuntimeProvider(for _: ChainModel.Id) -> RuntimeProviderProtocol? {
        nil
    }

    func switchSync(mode _: ChainSyncMode, chainId _: ChainModel.Id) throws {}

    func chainsSubscribe(
        _ target: AnyObject,
        runningInQueue _: DispatchQueue,
        updateClosure: @escaping ([DataProviderChange<ChainModel>]) -> Void
    ) {
        lock.withLock { state in
            state.subscriptions.append(SubscriptionRecord(target: target, closure: updateClosure))
        }
    }

    func chainsUnsubscribe(_ target: AnyObject) {
        lock.withLock { state in
            state.unsubscribeCount += 1
            state.subscriptions.removeAll { $0.target === target }
        }
    }

    func subscribeChainState(_: ConnectionStateSubscription, chainId _: ChainModel.Id) {}

    func unsubscribeChainState(_: ConnectionStateSubscription, chainId _: ChainModel.Id) {}

    func retainConnections(_: ConnectionRetainScope) -> ConnectionRetainToken {
        ConnectionRetainToken()
    }

    func syncUp() {}

    func deliverChainInsert(_ chain: ChainModel) {
        let closures = lock.withLock { state in
            state.subscriptions.map(\.closure)
        }

        let changes: [DataProviderChange<ChainModel>] = [.insert(newItem: chain)]

        for closure in closures {
            closure(changes)
        }
    }
}
