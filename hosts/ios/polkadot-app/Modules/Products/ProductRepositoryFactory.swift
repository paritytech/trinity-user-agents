import Foundation
import Operation_iOS
import Products

final class ProductRepositoryFactory {
    let storageFacade: StorageFacadeProtocol
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.operationQueue = operationQueue
        self.logger = logger
    }

    func createRepository() -> AnyDataProviderRepository<Product> {
        let mapper = ProductMapper()
        let repository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(mapper)
        )
        return AnyDataProviderRepository(repository)
    }

    func createProvider() -> StreamableProvider<Product> {
        let mapper = AnyCoreDataMapper(ProductMapper())

        let repository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: mapper
        )

        let repositoryObservable = CoreDataContextObservable(
            service: storageFacade.databaseService,
            mapper: mapper,
            predicate: { _ in true }
        )

        repositoryObservable.start { [logger] error in
            if let error {
                logger.error("Product observable error: \(error)")
            }
        }

        let source = AnyStreamableSource(EmptyStreamableSource<Product>())

        return StreamableProvider(
            source: source,
            repository: AnyDataProviderRepository(repository),
            observable: AnyDataProviderRepositoryObservable(repositoryObservable),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}
