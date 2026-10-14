import Foundation
import Operation_iOS
import Combine

public extension ChainRegistryProtocol {
    func chainsPublisher() -> AnyPublisher<[DataProviderChange<ChainModel>], Never> {
        Deferred {
            let publisher = CurrentValueSubject<[DataProviderChange<ChainModel>], Never>([])
            let subscriber = NSObject()
            return publisher
                .handleEvents(
                    receiveSubscription: { [weak self] _ in
                        self?.chainsSubscribe(
                            subscriber,
                            runningInQueue: .main
                        ) {
                            publisher.send($0)
                        }
                    },
                    receiveCancel: {
                        self.chainsUnsubscribe(subscriber)
                    }
                )
        }
        .eraseToAnyPublisher()
    }
}
