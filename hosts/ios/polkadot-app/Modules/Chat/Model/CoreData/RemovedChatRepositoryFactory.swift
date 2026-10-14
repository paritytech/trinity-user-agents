import Foundation
import Operation_iOS
import CoreData

protocol RemovedChatRepositoryMaking {
    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.RemovedChat>
}

final class RemovedChatRepositoryFactory: RemovedChatRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.RemovedChat> {
        AnyDataProviderRepository(storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(RemovedChatMapper())
        ))
    }
}
