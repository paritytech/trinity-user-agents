import Foundation
import Operation_iOS
import SubstrateStorageSubscription

protocol SubstrateDataProviderFactoryProtocol {
    func createStorageProvider(for key: String) -> StreamableProvider<ChainStorageItem>
}

final class SubstrateDataProviderFactory: SubstrateDataProviderFactoryProtocol {
    let facade: StorageFacadeProtocol
    let operationQueue: OperationQueue
    let logger: LoggerProtocol

    init(
        facade: StorageFacadeProtocol,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.facade = facade
        self.operationQueue = operationQueue
        self.logger = logger
    }

    func createStorageProvider(for key: String) -> StreamableProvider<ChainStorageItem> {
        let filter = NSPredicate.filterStorageItemsBy(identifier: key)
        let storage: CoreDataRepository<ChainStorageItem, CDChainStorageItem> =
            facade.createRepository(filter: filter)
        let source = EmptyStreamableSource<ChainStorageItem>()
        let observable = CoreDataContextObservable(
            service: facade.databaseService,
            mapper: AnyCoreDataMapper(storage.dataMapper),
            predicate: { $0.identifier == key }
        )

        observable.start { error in
            if let error {
                self.logger.error("Can't start storage observing: \(error)")
            }
        }

        return StreamableProvider(
            source: AnyStreamableSource(source),
            repository: AnyDataProviderRepository(storage),
            observable: AnyDataProviderRepositoryObservable(observable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}
