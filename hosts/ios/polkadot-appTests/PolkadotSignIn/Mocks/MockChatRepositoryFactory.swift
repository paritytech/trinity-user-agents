@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatRepositoryFactory: ChatRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.LocalModel>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available in device sync test mocks")
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalModel> {
        AnyDataProviderRepository(repository)
    }
}
