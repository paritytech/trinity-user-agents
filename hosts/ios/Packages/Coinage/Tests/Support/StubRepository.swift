import Foundation
import Operation_iOS
@testable import Coinage

/// Minimal in-memory repository for tests that only need a repository to exist. Fetches return the
/// seeded models; saves and deletes are no-ops.
final class StubRepository<T: Identifiable>: DataProviderRepositoryProtocol, @unchecked Sendable {
    typealias Model = T
    let models: [T]

    init(models: [T] = []) {
        self.models = models
    }

    func fetchOperation(
        by modelIdClosure: @escaping () throws -> String,
        options _: RepositoryFetchOptions
    ) -> BaseOperation<T?> {
        ClosureOperation { [models] in
            (try? modelIdClosure()).flatMap { id in
                models.first { $0.identifier == id }
            }
        }
    }

    func fetchAllOperation(with _: RepositoryFetchOptions) -> BaseOperation<[T]> {
        ClosureOperation { [models] in models }
    }

    func fetchOperation(by _: RepositorySliceRequest, options _: RepositoryFetchOptions) -> BaseOperation<[T]> {
        ClosureOperation { [models] in models }
    }

    func saveOperation(_: @escaping () throws -> [T], _: @escaping () throws -> [String]) -> BaseOperation<Void> {
        ClosureOperation {}
    }

    func replaceOperation(_: @escaping () throws -> [T]) -> BaseOperation<Void> {
        ClosureOperation {}
    }

    func fetchCountOperation() -> BaseOperation<Int> {
        ClosureOperation { [models] in models.count }
    }

    func deleteAllOperation() -> BaseOperation<Void> {
        ClosureOperation {}
    }
}
