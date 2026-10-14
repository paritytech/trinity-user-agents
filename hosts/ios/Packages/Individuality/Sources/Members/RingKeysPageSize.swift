import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency

public extension RuntimeCodingServiceProtocol {
    /// Number of keys a single `RingKeys` page holds.
    ///
    /// A member's `ringPosition` counts within its own page while `RingKeysStatus.included` counts across the
    /// whole ring, so the two are only comparable once the pages before the member's own are added back.
    func fetchRingKeysPageSize() async throws -> Int {
        let codingFactory = try await fetchCoderFactoryOperation().asyncExecute()

        let operation = StorageConstantOperation<MembersPallet.RingExponent>(
            path: MembersPallet.Constants.maxFlexibleRingExponent()
        )
        operation.codingFactory = codingFactory

        return try await operation.asyncExecute().ringCapacity
    }
}
