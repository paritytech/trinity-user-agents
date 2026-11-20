import Foundation
import SubstrateSdk
import SubstrateStorageQuery

public extension RuntimeCodingServiceProtocol {
    func fetchConstant<T: Decodable>(path: ConstantCodingPath) async throws -> T {
        let operation = StorageConstantOperation<T>(path: path, fallbackValue: nil)
        operation.codingFactory = try await fetchCoderFactoryOperation().asyncExecute()

        return try await operation.asyncExecute()
    }
}
