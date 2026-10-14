@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockLocalDeviceRepositoryFactory: LocalDeviceRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<Chat.LocalDevice>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available in device sync test mocks")
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<Chat.LocalDevice> {
        AnyDataProviderRepository(repository)
    }

    func save(_ devices: [Chat.LocalDevice]) async throws {
        try await repository.saveOperation({ devices }, { [] }).asyncExecute()
    }
}
