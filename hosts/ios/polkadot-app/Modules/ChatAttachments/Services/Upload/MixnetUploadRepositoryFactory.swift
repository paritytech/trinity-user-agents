import CoreData
import Foundation
import Operation_iOS

final class MixnetUploadRepositoryFactory {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository() -> AnyDataProviderRepository<MixnetUpload> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(MixnetUploadMapper())
            )
        )
    }

    func createUpdateRepository() -> AnyDataProviderRepository<MixnetUploadUpdate> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(MixnetUploadUpdateMapper())
            )
        )
    }
}
