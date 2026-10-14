import Foundation
import Operation_iOS
import CoreData

protocol PolkadotSignInHostRepositoryMaking {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<PolkadotSignInHost>
}

final class PolkadotSignInHostRepositoryFactory: PolkadotSignInHostRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    var databaseService: CoreDataServiceProtocol {
        storageFacade.databaseService
    }

    func createRepository(
        forFilter filter: NSPredicate?
    ) -> AnyDataProviderRepository<PolkadotSignInHost> {
        AnyDataProviderRepository(storageFacade.createRepository(
            filter: filter,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(PolkadotSignInHostMapper())
        ))
    }
}
