@testable import polkadot_app
import AsyncExtensions
import CoreData
import Foundation
import Foundation_iOS
import MessageExchangeKit
import Operation_iOS
import SubstrateSdk

final class MockOutgoingUpdateTimeRepository: DataProviderRepositoryProtocol {
    typealias Model = Chat.OutgoingUpdateTimeUpdate

    private let repository: InMemoryDataProviderRepository<Chat.OutgoingUpdateTimeUpdate>
    private let onSave: ([Chat.OutgoingUpdateTimeUpdate]) -> Void

    init(
        repository: InMemoryDataProviderRepository<Chat.OutgoingUpdateTimeUpdate>,
        onSave: @escaping ([Chat.OutgoingUpdateTimeUpdate]) -> Void
    ) {
        self.repository = repository
        self.onSave = onSave
    }

    func fetchOperation(
        by modelIdClosure: @escaping () throws -> String,
        options: RepositoryFetchOptions
    ) -> BaseOperation<Chat.OutgoingUpdateTimeUpdate?> {
        repository.fetchOperation(by: modelIdClosure, options: options)
    }

    func fetchAllOperation(with options: RepositoryFetchOptions) -> BaseOperation<[Chat.OutgoingUpdateTimeUpdate]> {
        repository.fetchAllOperation(with: options)
    }

    func fetchOperation(
        by request: RepositorySliceRequest,
        options: RepositoryFetchOptions
    ) -> BaseOperation<[Chat.OutgoingUpdateTimeUpdate]> {
        repository.fetchOperation(by: request, options: options)
    }

    func saveOperation(
        _ updateModelsBlock: @escaping () throws -> [Chat.OutgoingUpdateTimeUpdate],
        _ deleteIdsBlock: @escaping () throws -> [String]
    ) -> BaseOperation<Void> {
        repository.saveOperation({
            let updates = try updateModelsBlock()
            self.onSave(updates)
            return updates
        }, deleteIdsBlock)
    }

    func replaceOperation(
        _ newModelsBlock: @escaping () throws -> [Chat.OutgoingUpdateTimeUpdate]
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
