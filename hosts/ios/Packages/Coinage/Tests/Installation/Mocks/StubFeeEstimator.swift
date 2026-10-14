import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

final class StubFeeEstimator: RegistrationFeeEstimating, @unchecked Sendable {
    var fee: BigUInt = 30_000_000
    private(set) var estimates = 0

    func estimateFee(
        _: @escaping ExtrinsicBuilderClosure,
        origin _: any ExtrinsicOriginDefining
    ) async throws -> BigUInt {
        estimates += 1
        return fee
    }
}
