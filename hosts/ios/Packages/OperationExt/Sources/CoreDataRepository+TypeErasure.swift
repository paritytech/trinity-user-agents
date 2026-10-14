import Foundation
import Operation_iOS

public extension CoreDataRepository {
    func eraseType() -> AnyDataProviderRepository<T> {
        AnyDataProviderRepository(self)
    }
}

public extension CoreDataMapperProtocol {
    func eraseType() -> AnyCoreDataMapper<DataProviderModel, CoreDataEntity> {
        AnyCoreDataMapper(self)
    }
}
