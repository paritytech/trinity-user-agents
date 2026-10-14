import Foundation
import Operation_iOS
import CoreData

protocol ChatRepositoryMaking {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LocalModel>
}

final class ChatRepositoryFactory: ChatRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    var databaseService: CoreDataServiceProtocol {
        storageFacade.databaseService
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(ChatModelMapper())
            )
        )
    }
}
