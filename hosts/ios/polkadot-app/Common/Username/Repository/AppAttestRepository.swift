import Foundation
import Operation_iOS
import UniqueDevice

final class AppAttestRepositoryFactory: AppAttestRepositoryFactoryProtocol {
    let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade
    }

    func createAppAttestRepository() -> AnyDataProviderRepository<AppAttestLocalSettings> {
        let singleValueMapper = SingleValueMapper<AppAttestLocalSettings>()
        let repository = storageFacade.createRepository(mapper: AnyCoreDataMapper(singleValueMapper))
        return AnyDataProviderRepository(repository)
    }
}
