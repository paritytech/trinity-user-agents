import CoreData
import Foundation
import Operation_iOS
@testable import polkadot_app

/// Works like the test facade until told to refuse, then fails reads of a stored row or every write.
final class StoreRefusingStorageFacade: UserDataStorageTestFacade, @unchecked Sendable {
    struct ReadRefused: Error {}
    struct WriteRefused: Error {}

    private let refusal = StoreRefusal()

    /// Lets `allowedReads` more rows be read, then refuses every read after them.
    func refuseReads(after allowedReads: Int = 0) {
        refusal.refuseReads(after: allowedReads)
    }

    func refuseWrites() {
        refusal.refusesWrites = true
    }

    func allowWrites() {
        refusal.refusesWrites = false
    }

    override func createRepository<T, U>(
        filter: NSPredicate?,
        sortDescriptors: [NSSortDescriptor],
        mapper: AnyCoreDataMapper<T, U>
    ) -> CoreDataRepository<T, U>
        where T: Operation_iOS.Identifiable, U: NSManagedObject {
        CoreDataRepository(
            databaseService: WriteRefusingDatabaseService(wrapped: databaseService, refusal: refusal),
            mapper: AnyCoreDataMapper(ReadRefusingMapper(wrapping: mapper, refusal: refusal)),
            filter: filter,
            sortDescriptors: sortDescriptors
        )
    }
}
