import Foundation
import Operation_iOS

protocol RecentContactRepositoryFactoryProtocol: AnyObject {
    func createRecentContactsRepository() -> AnyDataProviderRepository<RecentContactModel>
}

final class RecentContactRepositoryFactory {
    let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade
    }
}

extension RecentContactRepositoryFactory: RecentContactRepositoryFactoryProtocol {
    func createRecentContactsRepository() -> AnyDataProviderRepository<RecentContactModel> {
        let mapper = RecentContactMapper()
        let repository = storageFacade.createRepository(mapper: AnyCoreDataMapper(mapper))

        return AnyDataProviderRepository(repository)
    }
}
