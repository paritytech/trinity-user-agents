import CoreData
import Foundation
import Operation_iOS

final class MixnetDownloadRepositoryFactory {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
    }

    func createRepository() -> AnyDataProviderRepository<MixnetDownload> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(MixnetDownloadMapper())
            )
        )
    }

    func createChunkIndexRepository() -> AnyDataProviderRepository<MixnetDownloadChunkIndex> {
        AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: nil,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(MixnetDownloadChunkIndexMapper())
            )
        )
    }
}
