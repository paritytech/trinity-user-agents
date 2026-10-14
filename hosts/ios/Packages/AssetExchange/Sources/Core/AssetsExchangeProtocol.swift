import Foundation
import Operation_iOS

public protocol AssetsExchangeProtocol {
    func availableDirectSwapConnections() -> CompoundOperationWrapper<[any AssetExchangableGraphEdge]>
}

public protocol AssetsExchangeProviding: AnyObject {
    func setup()
    func throttle()

    func subscribeExchanges(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping ([AssetsExchangeProtocol]) -> Void
    )

    func unsubscribeExchanges(_ target: AnyObject)

    func inject(graph: AssetsExchangeGraphProtocol)
}

public protocol AssetsExchangeGraphProviding: AnyObject {
    func setup()
    func throttle()

    func subscribeGraph(
        _ target: AnyObject,
        notifyingIn queue: DispatchQueue,
        onChange: @escaping (AssetsExchangeGraphProtocol?, AssetsExchangeGraphProviderStats) -> Void
    )

    func unsubscribeGraph(_ target: AnyObject)
}

public extension AssetsExchangeGraphProviding {
    func asyncWaitGraphWrapper(
        using workingQueue: DispatchQueue = .global()
    ) -> CompoundOperationWrapper<AssetsExchangeGraphProtocol> {
        let subscriber = NSObject()

        let operation = AsyncClosureOperation<AssetsExchangeGraphProtocol>(
            operationClosure: { [weak self] completion in
                self?.subscribeGraph(
                    subscriber,
                    notifyingIn: workingQueue
                ) { graph, _ in
                    self?.unsubscribeGraph(subscriber)

                    guard let graph else {
                        return
                    }

                    completion(.success(graph))
                }
            },
            cancelationClosure: { [weak self] in
                self?.unsubscribeGraph(subscriber)
            }
        )

        return CompoundOperationWrapper(targetOperation: operation)
    }
}
