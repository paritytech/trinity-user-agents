import Foundation
import os
import SubstrateSdk
@testable import Revive

/// Answers every call with `result` and keeps what it was asked.
final class StubReviveContractCalling: ReviveContractCalling, @unchecked Sendable {
    struct Call: Equatable {
        let chainId: ChainId
        let origin: AccountId
        let contract: EvmAddress
        let input: Data
        let blockHash: BlockHash?
    }

    struct MappingCheck: Equatable {
        let chainId: ChainId
        let account: AccountId
    }

    private let state: OSAllocatedUnfairLock<(calls: [Call], mappingChecks: [MappingCheck])>
    let result: Result<ReviveDryRunResult, Error>
    let mapped: Bool

    init(result: Result<ReviveDryRunResult, Error>, mapped: Bool = true) {
        self.result = result
        self.mapped = mapped
        state = OSAllocatedUnfairLock(initialState: ([], []))
    }

    var calls: [Call] { state.withLock { $0.calls } }
    var mappingChecks: [MappingCheck] { state.withLock { $0.mappingChecks } }

    func call(
        chainId: ChainId,
        origin: AccountId,
        contract: EvmAddress,
        input: Data,
        at blockHash: BlockHash?
    ) async throws -> ReviveDryRunResult {
        state.withLock {
            $0.calls.append(Call(
                chainId: chainId,
                origin: origin,
                contract: contract,
                input: input,
                blockHash: blockHash
            ))
        }
        return try result.get()
    }

    func isAccountMapped(chainId: ChainId, account: AccountId) async throws -> Bool {
        state.withLock { $0.mappingChecks.append(MappingCheck(chainId: chainId, account: account)) }
        return mapped
    }
}
