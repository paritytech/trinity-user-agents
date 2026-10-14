import Foundation
import Operation_iOS
import CoreData

protocol ChatContactRepositoryMaking {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.Contact>
}

final class ChatContactRepositoryFactory: ChatContactRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    var databaseService: CoreDataServiceProtocol {
        storageFacade.databaseService
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ChatContactMapper())
        ))
    }
}
