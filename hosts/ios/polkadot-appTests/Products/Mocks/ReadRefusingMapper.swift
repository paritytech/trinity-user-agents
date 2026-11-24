import CoreData
import Operation_iOS
@testable import polkadot_app

final class ReadRefusingMapper<T: Operation_iOS.Identifiable, U: NSManagedObject>: CoreDataMapperProtocol {
    private let wrapped: AnyCoreDataMapper<T, U>
    private let refusal: StoreRefusal

    init(wrapping wrapped: AnyCoreDataMapper<T, U>, refusal: StoreRefusal) {
        self.wrapped = wrapped
        self.refusal = refusal
    }

    var entityIdentifierFieldName: String {
        wrapped.entityIdentifierFieldName
    }

    func transform(entity: U) throws -> T {
        guard refusal.allowsRead() else {
            throw StoreRefusingStorageFacade.ReadRefused()
        }
        return try wrapped.transform(entity: entity)
    }

    func populate(entity: U, from model: T, using context: NSManagedObjectContext) throws {
        try wrapped.populate(entity: entity, from: model, using: context)
    }
}
