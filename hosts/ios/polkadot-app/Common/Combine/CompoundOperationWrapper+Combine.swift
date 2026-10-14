import Operation_iOS
import Combine
import Foundation

extension CompoundOperationWrapper {
    func publisher(
        in operationQueue: OperationQueue
    ) -> AnyPublisher<ResultType, Error> {
        Deferred { [self] in
            let callStore = CancellableCallStore()
            return Future<ResultType, Error> { promise in
                executeCancellable(
                    wrapper: self,
                    inOperationQueue: operationQueue,
                    backingCallIn: callStore,
                    runningCallbackIn: .main
                ) { result in
                    promise(result)
                }
            }
            .handleEvents(receiveCancel: {
                callStore.cancel()
            })
        }
        .eraseToAnyPublisher()
    }
}
