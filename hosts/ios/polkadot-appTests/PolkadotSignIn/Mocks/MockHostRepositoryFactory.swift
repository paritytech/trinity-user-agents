import Foundation
import Operation_iOS

@testable import polkadot_app

final class MockHostRepositoryFactory: PolkadotSignInHostRepositoryMaking {
    private let repository = InMemoryDataProviderRepository<PolkadotSignInHost>()

    var databaseService: CoreDataServiceProtocol {
        fatalError("Core Data is not available to the in-memory host repository")
    }

    func createRepository(forFilter _: NSPredicate?) -> AnyDataProviderRepository<PolkadotSignInHost> {
        AnyDataProviderRepository(repository)
    }

    func fetchAll() async throws -> [PolkadotSignInHost] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }
}
