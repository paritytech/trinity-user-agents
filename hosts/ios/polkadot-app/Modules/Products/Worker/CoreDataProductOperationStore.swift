import Foundation
import Operation_iOS
import AsyncExtensions
import Products

/// CoreData-backed persistence for open worker operations, in the shared
/// `UserDataModel` store behind the `CDProductOperation` entity.
///
/// The persisted set is the source of truth for worker keep-alive:
/// `ProductWorkerOperationService` subscribes to it and holds a worker lock per
/// operation, so operations survive a relaunch and restore their workers.
final class CoreDataProductOperationStore: ProductOperationStoring, @unchecked Sendable {
    private let storageFacade: StorageFacadeProtocol
    private let mapper = AnyCoreDataMapper(ProductOperationMapper())
    private let repository: AnyDataProviderRepository<ProductOperationRecord>

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        self.storageFacade = storageFacade
        repository = AnyDataProviderRepository(storageFacade.createRepository(mapper: mapper))
    }

    func save(_ record: ProductOperationRecord) async throws {
        try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
    }

    func delete(productId: ProductId, id: UInt32) async throws {
        let identifier = ProductOperationRecord.identifier(productId: productId, id: id)
        try await repository.saveOperation({ [] }, { [identifier] }).asyncExecute()
    }

    func all() async throws -> [ProductOperationRecord] {
        try await repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    func clearAll() async throws {
        try await repository.deleteAllOperation().asyncExecute()
    }

    func subscribe() -> AnyAsyncSequence<[ProductOperationRecord]> {
        storageFacade.subscribeSnapshot(mapper: mapper)
    }
}
