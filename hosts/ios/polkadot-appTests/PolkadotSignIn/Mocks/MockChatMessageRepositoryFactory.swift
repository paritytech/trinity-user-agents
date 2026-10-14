@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatMessageRepositoryFactory: ChatMessageRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.LocalMessage>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available in device sync test mocks")
    }

    func createRepository(forFilter filter: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalMessage> {
        createRepository(forFilter: filter, sortDescriptors: [])
    }

    func createRepository(
        forFilter _: NSPredicate?,
        sortDescriptors _: [NSSortDescriptor]
    ) -> AnyDataProviderRepository<Chat.LocalMessage> {
        AnyDataProviderRepository(repository)
    }

    func fetchMessages() async throws -> [Chat.LocalMessage] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }

    func save(_ messages: [Chat.LocalMessage]) async throws {
        try await repository.saveOperation({ messages }, { [] }).asyncExecute()
    }
}
