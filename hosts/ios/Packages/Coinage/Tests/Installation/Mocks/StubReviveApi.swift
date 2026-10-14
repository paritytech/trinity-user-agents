import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import Revive
import SubstrateSdk
@testable import Coinage

final class StubReviveApi: ReviveContractApiProtocol, @unchecked Sendable {
    var mapped = true
    var dryRunResult: Result<ReviveDryRun, Error> = .success(
        ReviveDryRun(
            data: Data(),
            weightRequired: Substrate.WeightV2(refTime: 1_000_000, proofSize: 70_000),
            storageDeposit: 413_000_000
        )
    )
    private(set) var dryRuns: [(origin: AccountId, contract: EvmAddress, input: Data)] = []
    private(set) var mappingChecks: [AccountId] = []

    func callReadOnly(contract _: EvmAddress, input _: Data, at _: Data?) async throws -> Data { Data() }

    func dryRun(origin: AccountId, contract: EvmAddress, input: Data) async throws -> ReviveDryRun {
        dryRuns.append((origin, contract, input))
        return try dryRunResult.get()
    }

    func isAccountMapped(_ account: AccountId) async throws -> Bool {
        mappingChecks.append(account)
        return mapped
    }
}
