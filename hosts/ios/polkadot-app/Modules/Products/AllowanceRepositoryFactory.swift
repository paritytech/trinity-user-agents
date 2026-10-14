import Foundation
import CoreData
import Operation_iOS
import Individuality

final class AllowanceRepositoryFactory {
    private let storageFacade: StorageFacadeProtocol

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade
    }

    func createPGASRepository() -> AnyDataProviderRepository<AllowanceRecord> {
        createRepository(for: .pgas)
    }

    func createStatementStoreRepository() -> AnyDataProviderRepository<AllowanceRecord> {
        createRepository(for: .statementStore)
    }

    private func createRepository(for kind: AllowanceRecord.Kind) -> AnyDataProviderRepository<AllowanceRecord> {
        let filter = NSPredicate(
            format: "%K == %d",
            #keyPath(CDAllowanceRecord.kind),
            Int(kind.persistenceCode)
        )
        return AnyDataProviderRepository(
            storageFacade.createRepository(
                filter: filter,
                sortDescriptors: [],
                mapper: AnyCoreDataMapper(AllowanceRecordMapper())
            )
        )
    }
}
