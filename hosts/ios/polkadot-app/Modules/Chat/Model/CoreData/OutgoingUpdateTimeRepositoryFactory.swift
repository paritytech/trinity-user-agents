import Foundation
import Operation_iOS
import CoreData

protocol OutgoingUpdateTimeRepositoryMaking {
    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.OutgoingUpdateTimeUpdate>
}

final class OutgoingUpdateTimeRepositoryFactory: OutgoingUpdateTimeRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.OutgoingUpdateTimeUpdate> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(OutgoingUpdateTimeMapper())
            )
        )
    }
}
