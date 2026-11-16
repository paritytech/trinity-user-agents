import CoreData
import Operation_iOS
@testable import polkadot_app

/// Fails writes at the service, since a deletion never reaches the mapper.
struct WriteRefusingDatabaseService: CoreDataServiceProtocol {
    let wrapped: CoreDataServiceProtocol
    let refusal: StoreRefusal

    var configuration: CoreDataServiceConfigurationProtocol {
        wrapped.configuration
    }

    func performWrite<T>(_ block: @escaping CoreDataContextBlock<T>, completion: @escaping CoreDataResultBlock<T>) {
        guard !refusal.refusesWrites else {
            completion(.failure(StoreRefusingStorageFacade.WriteRefused()))
            return
        }
        wrapped.performWrite(block, completion: completion)
    }

    func performRead<T>(_ block: @escaping CoreDataContextBlock<T>, completion: @escaping CoreDataResultBlock<T>) {
        wrapped.performRead(block, completion: completion)
    }

    func performAsync(block: @escaping CoreDataContextInvocationBlock) {
        wrapped.performAsync(block: block)
    }

    func performObserve(block: @escaping CoreDataContextInvocationBlock) {
        wrapped.performObserve(block: block)
    }

    func performWithObserver(block: @escaping CoreDataWriterObserverBlock) {
        wrapped.performWithObserver(block: block)
    }

    func close() throws {
        try wrapped.close()
    }

    func drop() throws {
        try wrapped.drop()
    }
}
