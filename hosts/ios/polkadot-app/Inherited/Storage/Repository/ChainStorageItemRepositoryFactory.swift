import Foundation
import Operation_iOS
import SubstrateStorageSubscription

protocol OnChainStorageRepositoryFactoryProtocol {
    func createChainStorageItemRepository() -> AnyDataProviderRepository<ChainStorageItem>
    func createChainStorageItemRepository(filter: NSPredicate) -> AnyDataProviderRepository<ChainStorageItem>
}

final class OnChainStorageRepositoryFactory {
    let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade
    }
}

extension OnChainStorageRepositoryFactory: OnChainStorageRepositoryFactoryProtocol {
    func createChainStorageItemRepository() -> AnyDataProviderRepository<ChainStorageItem> {
        let repository: CoreDataRepository<ChainStorageItem, CDChainStorageItem> =
            storageFacade.createRepository()

        return AnyDataProviderRepository(repository)
    }

    func createChainStorageItemRepository(
        filter: NSPredicate
    ) -> AnyDataProviderRepository<ChainStorageItem> {
        let repository: CoreDataRepository<ChainStorageItem, CDChainStorageItem> =
            storageFacade.createRepository(filter: filter)

        return AnyDataProviderRepository(repository)
    }
}
