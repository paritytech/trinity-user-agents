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

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available in device sync test mocks")
    }

    var deletedIds: [String] {
        lock.withLock { _deletedIds }
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.Contact> {
        AnyDataProviderRepository(MockChatContactRepository(
            repository: repository,
            onDelete: { [weak self] ids in
                self?.lock.withLock { self?._deletedIds.append(contentsOf: ids) }
            }
        ))
    }
}
