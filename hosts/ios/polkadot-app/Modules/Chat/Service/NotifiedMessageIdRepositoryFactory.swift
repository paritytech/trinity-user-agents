import CoreData
import Foundation
import Operation_iOS

protocol NotifiedMessageIdRepositoryMaking {
    func createRepository() -> AnyDataProviderRepository<Chat.NotifiedMessageId>
}

final class NotifiedMessageIdRepositoryFactory: NotifiedMessageIdRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository() -> AnyDataProviderRepository<Chat.NotifiedMessageId> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(NotifiedMessageIdMapper())
            )
        )
    }
}
