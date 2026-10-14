import CoreData
import Foundation
import Operation_iOS

final class MapperCallCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    var value: Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }

    func increment() {
        lock.lock()
        count += 1
        lock.unlock()
    }
}

/// Wraps a production mapper and counts `transform(entity:)` calls, so re-map amplification is a
/// number in the report rather than an estimate.
final class CountingMapper<Base: CoreDataMapperProtocol>: CoreDataMapperProtocol {
    typealias DataProviderModel = Base.DataProviderModel
    typealias CoreDataEntity = Base.CoreDataEntity

    private let base: Base
    private let counter: MapperCallCounter

    init(_ base: Base, counter: MapperCallCounter) {
        self.base = base
        self.counter = counter
    }

    var entityIdentifierFieldName: String {
        base.entityIdentifierFieldName
    }

    func transform(entity: CoreDataEntity) throws -> DataProviderModel {
        counter.increment()
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
