@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatContactRepositoryFactory: ChatContactRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.Contact>()
    private let lock = NSLock()
    private var _deletedIds = [String]()
    private var _filters = [NSPredicate?]()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available in device sync test mocks")
    }

    var deletedIds: [String] {
        lock.withLock { _deletedIds }
    }

    var filters: [NSPredicate?] {
        lock.withLock { _filters }
    }

    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.Contact> {
        lock.withLock { _filters.append(filter) }
        return AnyDataProviderRepository(MockChatContactRepository(
            repository: repository,
            onDelete: { [weak self] ids in
                self?.lock.withLock { self?._deletedIds.append(contentsOf: ids) }
            }
        ))
    }
}
