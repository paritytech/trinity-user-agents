import CoreData
import Foundation
import Operation_iOS

protocol SSOHandledRequestRepositoryMaking {
    func createRepository() -> AnyDataProviderRepository<SSOHandledRequest>
}

final class SSOHandledRequestRepositoryFactory: SSOHandledRequestRepositoryMaking {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository() -> AnyDataProviderRepository<SSOHandledRequest> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(SSOHandledRequestMapper())
            )
        )
    }
}
