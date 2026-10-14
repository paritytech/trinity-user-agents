@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockRemovedChatRepositoryFactory: RemovedChatRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.RemovedChat>()

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.RemovedChat> {
        AnyDataProviderRepository(repository)
    }

    func fetchRemovedChats() async throws -> [Chat.RemovedChat] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }
}
