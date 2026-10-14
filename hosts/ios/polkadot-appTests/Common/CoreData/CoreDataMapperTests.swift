import BigInt
import Coinage
import Foundation
import Operation_iOS
import Testing
import CoreData

@testable import polkadot_app

@Suite("CoreDataMapperTests")
enum CoreDataMapperTests {}

// MARK: - Helpers

extension UserDataStorageTestFacade {
    func makeRepo<M: CoreDataMapperProtocol>(mapper: M) -> AnyDataProviderRepository<M.DataProviderModel>
        where M.DataProviderModel: Identifiable, M.CoreDataEntity: NSManagedObject {
        AnyDataProviderRepository(
            createRepository(filter: nil, sortDescriptors: [], mapper: AnyCoreDataMapper(mapper))
        )
    }
}
