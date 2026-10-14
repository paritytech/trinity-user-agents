import CoreData
import Foundation
import Operation_iOS

/// Wraps a real mapper and counts `transform(entity:)` calls; behaviour is the wrapped mapper's.
final class CountingCoreDataMapper<Base: CoreDataMapperProtocol>: CoreDataMapperProtocol {
    typealias DataProviderModel = Base.DataProviderModel
    typealias CoreDataEntity = Base.CoreDataEntity

    private let base: Base
    private let lock = NSLock()
    private var count = 0

    init(_ base: Base) {
        self.base = base
    }

    var transformCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }

    var entityIdentifierFieldName: String {
        base.entityIdentifierFieldName
    }

    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        lock.lock()
        count += 1
        lock.unlock()

        return try base.transform(entity: entity)
    }

    func populate(
        entity: CoreDataEntity,
        from model: DataProviderModel,
        using context: NSManagedObjectContext
    ) throws {
        try base.populate(entity: entity, from: model, using: context)
    }
}
