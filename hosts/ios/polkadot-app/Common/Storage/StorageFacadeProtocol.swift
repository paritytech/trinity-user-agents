import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import AsyncExtensions
import SDKLogger

protocol StorageFacadeProtocol: AnyObject {
    var databaseService: CoreDataServiceProtocol { get }

    func createRepository<T, U>(
        filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor],
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U>
        where T: Identifiable, U: NSManagedObject
}

extension StorageFacadeProtocol {
    func createRepository<T, U>(
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U> where T: Identifiable, U: NSManagedObject {
        createRepository(filter: nil, sortDescriptors: [], mapper: mapper)
    }

    func createRepository<T, U>()
        -> CoreDataRepository<T, U> where T: Identifiable & Codable, U: NSManagedObject & CoreDataCodable {
        let mapper = AnyCoreDataMapper(CodableCoreDataMapper<T, U>())
        return createRepository(filter: nil, sortDescriptors: [], mapper: mapper)
    }

    func createRepository<T, U>(
        filter: NSPredicate
    ) -> CoreDataRepository<T, U> where T: Identifiable & Codable, U: NSManagedObject & CoreDataCodable {
        let mapper = AnyCoreDataMapper(CodableCoreDataMapper<T, U>())
        return createRepository(filter: filter, sortDescriptors: [], mapper: mapper)
    }

    func createRepository<T, U>(
        sortDescriptors: [NSSortDescriptor]
    ) -> CoreDataRepository<T, U> where T: Identifiable & Codable, U: NSManagedObject & CoreDataCodable {
        let mapper = AnyCoreDataMapper(CodableCoreDataMapper<T, U>())
        return createRepository(filter: nil, sortDescriptors: sortDescriptors, mapper: mapper)
    }

    func createRepository<T, U>(
        filter: NSPredicate,
        sortDescriptors: [NSSortDescriptor]
    ) -> CoreDataRepository<T, U> where T: Identifiable & Codable, U: NSManagedObject & CoreDataCodable {
        let mapper = AnyCoreDataMapper(CodableCoreDataMapper<T, U>())
        return createRepository(filter: filter, sortDescriptors: sortDescriptors, mapper: mapper)
    }

    func subscribeSnapshot<T>(
        mapper: AnyCoreDataMapper<T, some NSManagedObject>,
        filter: NSPredicate? = nil,
        logger: SDKLoggerProtocol? = nil,
        transform: @escaping ([T]) -> [T] = { $0 }
    ) -> AnyAsyncSequence<[T]> where T: Identifiable {
        databaseService.subscribeSnapshot(
            mapper: mapper,
            filter: filter,
            logger: logger,
            transform: transform
        )
    }

    func subscribeSingle<T>(
        mapper: AnyCoreDataMapper<T, some NSManagedObject>,
        filter: NSPredicate,
        logger: SDKLoggerProtocol? = nil
    ) -> AnyAsyncSequence<T?> where T: Identifiable {
        databaseService.subscribeSingle(
            mapper: mapper,
            filter: filter,
            logger: logger
        )
    }
}
