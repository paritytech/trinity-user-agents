import Foundation
import Individuality
import SubstrateSdk

final class MockAllowanceManager: AllowanceManaging {
    var allocateCallCount = 0
    var allocateError: Error?
    var releaseCallCount = 0
    var releaseError: Error?

    func allocate(
        accountId _: AccountId,
        policy _: OnExistingAllowancePolicy,
        priority _: AllowanceRecord.Priority,
        domain _: AllowanceRecord.Domain
    ) async throws {
        allocateCallCount += 1

        if let allocateError {
            throw allocateError
        }
    }

    func release(with _: AllowanceReleaseStrategy) async throws {
        releaseCallCount += 1

        if let releaseError {
            throw releaseError
        }
    }
}
