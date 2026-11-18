import CoreData
import Foundation
import Operation_iOS

extension CoreDataServiceProtocol {
    /// A one-shot read that blocks the caller until it is done, for the synchronous callers that build
    /// the shared runtime. Never call it from a context queue of this service.
    func performReadAndWait<T>(_ block: @escaping (NSManagedObjectContext) throws -> T) throws -> T {
        let semaphore = DispatchSemaphore(value: 0)
        var result: Result<T, Error>?

        performRead(block) { readResult in
            result = readResult
            semaphore.signal()
        }
        semaphore.wait()

        guard let result else { throw CoreDataServiceError.contextUnavailable }
        return try result.get()
    }
}
