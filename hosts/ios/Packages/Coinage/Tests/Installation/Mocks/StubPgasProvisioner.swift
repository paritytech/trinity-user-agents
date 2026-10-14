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

final class StubPgasProvisioner: PGASAccountProvisioning, @unchecked Sendable {
    var fundingError: Error?
    var coverError: Error?
    private(set) var funded: [AccountId] = []
    private(set) var covered: [(account: AccountId, required: BigUInt)] = []

    func ensureFunded(account: AccountId) async throws {
        funded.append(account)
        if let fundingError { throw fundingError }
    }

    func ensureCovers(account: AccountId, required: BigUInt) async throws {
        covered.append((account, required))
        if let coverError { throw coverError }
    }
}
