import Foundation
import Operation_iOS
import CoreData

protocol ChatMessageRepositoryMaking {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalMessage>
    func createRepository(
        forFilter filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor]
    ) -> AnyDataProviderRepository<Chat.LocalMessage>
}

final class ChatMessageRepositoryFactory: ChatMessageRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    var databaseService: CoreDataServiceProtocol {
        storageFacade.databaseService
    }

    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalMessage> {
        createRepository(forFilter: filter, sortDescriptors: [])
    }

    func createRepository(
        forFilter filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor]
    ) -> AnyDataProviderRepository<Chat.LocalMessage> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: sortDescriptors,
                mapper: AnyCoreDataMapper(ChatMessageEntityMapper())
            )
        )
    }
}
