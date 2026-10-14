import Foundation
import Operation_iOS
import CoreData

protocol LastSyncOfferIdRepositoryMaking {
    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LastSyncOfferIdUpdate>
}

final class LastSyncOfferIdRepositoryFactory: LastSyncOfferIdRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LastSyncOfferIdUpdate> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(LastSyncOfferIdMapper())
            )
        )
    }
}
