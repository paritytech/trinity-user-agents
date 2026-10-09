import Foundation
import Operation_iOS

@testable import polkadot_app

final class InMemoryHandledRequestRepositoryFactory: SSOHandledRequestRepositoryMaking {
    let repository = InMemoryDataProviderRepository<SSOHandledRequest>()

    func createRepository() -> AnyDataProviderRepository<SSOHandledRequest> {
        AnyDataProviderRepository(repository)
    }

    func fetchAll() async throws -> [SSOHandledRequest] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }

    func seed(messageId: String) async throws {
        try await repository
            .saveOperation({ [SSOHandledRequest(messageId: messageId)] }, { [] })
            .asyncExecute()
    }
}
