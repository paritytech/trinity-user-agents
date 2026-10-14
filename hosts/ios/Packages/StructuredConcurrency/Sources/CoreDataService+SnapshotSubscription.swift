import Foundation
import CoreData
import Operation_iOS
import OperationExt
import AsyncExtensions
import SDKLogger

public extension CoreDataServiceProtocol {
    func subscribeSnapshot<T: Identifiable, U: NSManagedObject>(
        mapper: AnyCoreDataMapper<T, U>,
        filter: NSPredicate? = nil,
        logger: SDKLoggerProtocol? = nil,
        transform: @escaping ([T]) -> [T] = { $0 }
    ) -> AnyAsyncSequence<[T]> {
        let service = self
        let syncQueue = DispatchQueue(label: "io.coredata.snapshot.subscription")

        let holder = AnyObjectHolder<CoreDataSnapshotSubscriber<T, U>>()

        return AsyncStream<[T]> { continuation in
            let request = NSFetchRequest<U>(entityName: String(describing: U.self))
            request.predicate = filter
            request.sortDescriptors = []

            let subscriber = CoreDataSnapshotSubscriber(
                service: service,
                mapper: mapper,
                fetchRequest: request,
                callbackQueue: syncQueue,
                logger: logger,
                transform: transform,
                onUpdate: { models in
                    continuation.yield(models)
                },
                onError: { _ in
                    continuation.finish()
                }
            )

            continuation.onTermination = { @Sendable _ in
                holder.set(nil)
            }

            holder.set(subscriber)
            subscriber.start()
        }
        .eraseToAnyAsyncSequence()
    }

    func subscribeSingle<T: Identifiable>(
        mapper: AnyCoreDataMapper<T, some NSManagedObject>,
        filter: NSPredicate,
        logger: SDKLoggerProtocol? = nil
    ) -> AnyAsyncSequence<T?> {
        subscribeSnapshot(mapper: mapper, filter: filter, logger: logger)
            .map(\.first)
            .eraseToAnyAsyncSequence()
    }
}
