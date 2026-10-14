import Foundation
import SubstrateSdk
import Operation_iOS

public extension AssetsExchangeServiceProtocol {
    func subscribeUpdates() -> AsyncStream<AssetsExchangeGraphProviderStats> {
        let target = NSObject()
        let syncQueue = DispatchQueue(label: "io.assets.exchange.service.async.updates")

        return AsyncStream { [weak self] continuation in
            guard let self else {
                continuation.finish()
                return
            }

            continuation.onTermination = { [weak self] _ in
                self?.unsubscribeUpdates(for: target)
            }

            subscribeUpdates(
                for: target,
                notifyingIn: syncQueue
            ) { stats in
                continuation.yield(stats)
            }
        }
    }

    func submit(
        using estimation: AssetExchangeFee,
        creditingTo accountId: AccountId?
    ) -> AsyncStream<AssetsExchangeAsyncSubmitEvent> {
        let syncQueue = DispatchQueue(label: "io.assets.exchange.service.async.submit")
        let operationQueue = OperationQueue()

        return AsyncStream { [weak self] continuation in
            guard let self else {
                return
            }

            let callStore = CancellableCallStore()

            let wrapper = submit(
                using: estimation,
                creditingTo: accountId,
                notifyingIn: syncQueue
            ) { index in
                continuation.yield(.inProgress(index))
            }

            continuation.onTermination = { [weak callStore] termination in
                guard termination == .cancelled else {
                    return
                }

                callStore?.cancel()
            }

            executeCancellable(
                wrapper: wrapper,
                inOperationQueue: operationQueue,
                backingCallIn: callStore,
                runningCallbackIn: syncQueue
            ) { result in
                switch result {
                case let .success(value):
                    continuation.yield(.completed(value))
                case let .failure(error):
                    continuation.yield(.failure(error))
                }

                continuation.finish()
            }
        }
    }
}

public enum AssetsExchangeAsyncSubmitEvent {
    case inProgress(Int)
    case completed(Balance)
    case failure(Error)
}
