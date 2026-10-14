import Foundation
import Operation_iOS
import OperationExt
import os

public extension ChainRegistryProtocol {
    func asyncChains(
        for chainIDs: Set<ChainModel.Id>,
        workQueue: DispatchQueue = .global()
    ) -> CompoundOperationWrapper<[ChainModel.Id: ChainModel]> {
        let subscriptionId = NSObject()

        let operation = AsyncClosureOperation<[ChainModel.Id: ChainModel]>(operationClosure: { [weak self] closure in
            self?.chainsSubscribe(
                subscriptionId,
                runningInQueue: workQueue
            ) { changes in
                guard !changes.isEmpty else {
                    return
                }

                self?.chainsUnsubscribe(subscriptionId)
                let allChains = changes.allChangedItems()

                let chainIDsWithChainModel = allChains.filter { chainIDs.contains($0.chainId) }
                    .reduce(into: [ChainModel.Id: ChainModel]()) {
                        $0[$1.chainId] = $1
                    }
                closure(.success(chainIDsWithChainModel))
            }
        }, cancelationClosure: { [weak self] in
            self?.chainsUnsubscribe(subscriptionId)
        })

        return CompoundOperationWrapper(targetOperation: operation)
    }

    func asyncWaitChainsSetup(for chainIds: Set<ChainModel.Id>) async {
        guard !chainIds.isEmpty else { return }

        let syncQueue = DispatchQueue(label: "io.chain.registry.wait.chain.setup")
        let waiter = ChainsSetupWaiter()

        await withTaskCancellationHandler {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                waiter.start(
                    chainIds: chainIds,
                    registry: self,
                    continuation: continuation,
                    syncQueue: syncQueue
                )
            }
        } onCancel: {
            waiter.cancel()
        }
    }
}

private final class ChainsSetupWaiter: @unchecked Sendable {
    private struct Subscription {
        let target: NSObject
        let registry: ChainRegistryProtocol
        let continuation: CheckedContinuation<Void, Never>
        var availableChains: [ChainModel.Id: ChainModel] = [:]
    }

    private enum WaiterState {
        case initial
        case active(Subscription)
        case done
    }

    private let stateLock = OSAllocatedUnfairLock<WaiterState>(initialState: .initial)

    func start(
        chainIds: Set<ChainModel.Id>,
        registry: ChainRegistryProtocol,
        continuation: CheckedContinuation<Void, Never>,
        syncQueue: DispatchQueue
    ) {
        let target = NSObject()

        let shouldSubscribe = stateLock.withLock { state -> Bool in
            guard case .initial = state else { return false }

            state = .active(
                Subscription(
                    target: target,
                    registry: registry,
                    continuation: continuation
                )
            )

            return true
        }

        guard shouldSubscribe else {
            // Already cancelled before we could subscribe
            continuation.resume()
            return
        }

        registry.chainsSubscribe(target, runningInQueue: syncQueue) { [weak self] changes in
            guard let self else { return }
            guard apply(changes, requiring: chainIds) else { return }
            finish()
        }
    }

    func cancel() {
        finish()
    }
}

private extension ChainsSetupWaiter {
    /// Folds the changes into the active state and reports whether every required chain is now available.
    func apply(
        _ changes: [DataProviderChange<ChainModel>],
        requiring chainIds: Set<ChainModel.Id>
    ) -> Bool {
        stateLock.withLock { state -> Bool in
            guard case var .active(subscription) = state else {
                return false
            }

            for change in changes {
                switch change {
                case let .insert(chain),
                     let .update(chain):
                    subscription.availableChains[chain.chainId] = chain
                case let .delete(chainId):
                    subscription.availableChains[chainId] = nil
                }
            }

            state = .active(subscription)

            return chainIds.allSatisfy { subscription.availableChains[$0] != nil }
        }
    }

    /// Single exit point for both completion and cancellation: the state read and the `.done`
    /// transition share one lock acquisition, so unsubscribe and resume happen at most once.
    func finish() {
        let subscription = stateLock.withLock { state -> Subscription? in
            defer { state = .done }

            guard case let .active(subscription) = state else {
                return nil
            }

            return subscription
        }

        guard let subscription else { return }

        subscription.registry.chainsUnsubscribe(subscription.target)
        subscription.continuation.resume()
    }
}
