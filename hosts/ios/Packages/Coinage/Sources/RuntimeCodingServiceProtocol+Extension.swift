import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency

extension RuntimeCodingServiceProtocol {
    func fetchConstant<T: LosslessStringConvertible & Equatable>(
        path: ConstantCodingPath,
        type _: T.Type
    ) async throws -> T {
        let codingFactory = try await fetchCoderFactoryOperation().asyncExecute()
        let operation = PrimitiveConstantOperation<T>(path: path)
        operation.codingFactory = codingFactory
        return try await operation.asyncExecute()
    }
}
