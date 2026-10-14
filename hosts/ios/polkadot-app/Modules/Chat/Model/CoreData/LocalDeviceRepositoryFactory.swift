import Foundation
import Operation_iOS
import CoreData

protocol LocalDeviceRepositoryMaking {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LocalDevice>
}

final class LocalDeviceRepositoryFactory: LocalDeviceRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    var databaseService: CoreDataServiceProtocol {
        storageFacade.databaseService
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<Chat.LocalDevice> {
        AnyDataProviderRepository(storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(LocalDeviceMapper())
        ))
    }
}
