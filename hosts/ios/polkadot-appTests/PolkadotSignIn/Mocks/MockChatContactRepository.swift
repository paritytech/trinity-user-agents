@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockChatContactRepository: DataProviderRepositoryProtocol {
    typealias Model = Chat.Contact

    private let repository: InMemoryDataProviderRepository<Chat.Contact>
    private let onDelete: ([String]) -> Void

    init(
        repository: InMemoryDataProviderRepository<Chat.Contact>,
        onDelete: @escaping ([String]) -> Void
    ) {
        self.repository = repository
        self.onDelete = onDelete
    }

    func fetchOperation(
        by modelIdClosure: @escaping () throws -> String,
        options: RepositoryFetchOptions
    ) -> BaseOperation<Chat.Contact?> {
        repository.fetchOperation(by: modelIdClosure, options: options)
    }

    func fetchAllOperation(with options: RepositoryFetchOptions) -> BaseOperation<[Chat.Contact]> {
        repository.fetchAllOperation(with: options)
    }

    func fetchOperation(
        by request: RepositorySliceRequest,
        options: RepositoryFetchOptions
    ) -> BaseOperation<[Chat.Contact]> {
        repository.fetchOperation(by: request, options: options)
    }

    func saveOperation(
        _ updateModelsBlock: @escaping () throws -> [Chat.Contact],
        _ deleteIdsBlock: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        repository.saveOperation(updateModelsBlock) {
            let ids = try deleteIdsBlock()
            self.onDelete(ids)
            return ids
        }
    }

    func replaceOperation(
        _ newModelsBlock: @escaping () throws -> [Chat.Contact]
    ) -> BaseOperation<Void> {
        repository.replaceOperation(newModelsBlock)
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        repository.fetchCountOperation()
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        repository.deleteAllOperation()
    }
}
