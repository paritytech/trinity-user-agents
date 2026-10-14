import CoreData
import Foundation
import Operation_iOS

public extension CoreDataServiceProtocol {
    /// Legacy bridge: runs `block` on the writer context and hands it out raw, so the block owns
    /// `save()` / `rollback()`. Prefer `performWrite` / `performRead`; this stays for callers not yet
    /// migrated and must leave no unsaved changes behind.
    func perform<T>(
        _ block: @escaping (NSManagedObjectContext) throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            performAsync { context, error in
                guard let context else {
                    continuation.resume(throwing: error ?? CoreDataRepositoryError.undefined)
                    return
                }
                context.perform {
                    do {
                        let result = try block(context)
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
        }
    }

    /// One transaction on the writer: saved when the block leaves changes, rolled back and rethrown
    /// when it throws. Writes are serialized in call order.
    func performWrite<T>(
        _ block: @escaping (NSManagedObjectContext) throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            performWrite(block) { continuation.resume(with: $0) }
        }
    }

    /// One-shot read on a reader context; may overlap the writer and other reads. Do not mutate the context.
    func performRead<T>(
        _ block: @escaping (NSManagedObjectContext) throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            performRead(block) { continuation.resume(with: $0) }
        }
    }
}
