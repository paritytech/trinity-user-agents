import Foundation
import os
import Revive
import SubstrateSdk

/// Answers read-only calls per contract address and keeps every call, so a test can see which
/// contract was read and how often.
final class StubReviveContractApi: ReviveContractApiProtocol, @unchecked Sendable {
    struct Call: Equatable {
        let contract: EvmAddress
        let input: Data
    }

    private let state = OSAllocatedUnfairLock(initialState: [Call]())
    private let answers: OSAllocatedUnfairLock<[EvmAddress: @Sendable (Data) throws -> Data]>

    init(answers: [EvmAddress: @Sendable (Data) throws -> Data] = [:]) {
        self.answers = OSAllocatedUnfairLock(initialState: answers)
    }

    var calls: [Call] { state.withLock { $0 } }

    func calls(to contract: EvmAddress) -> [Call] {
        calls.filter { $0.contract == contract }
    }

    func answer(_ contract: EvmAddress, with handler: @escaping @Sendable (Data) throws -> Data) {
        answers.withLock { $0[contract] = handler }
    }

    func callReadOnly(contract: EvmAddress, input: Data, at _: Data?) async throws -> Data {
        state.withLock { $0.append(Call(contract: contract, input: input)) }
        guard let handler = answers.withLock({ $0[contract] }) else {
            throw StubReviveContractApiError.unknownContract(contract)
        }
        return try handler(input)
    }

    func dryRun(origin _: AccountId, contract: EvmAddress, input _: Data) async throws -> ReviveDryRun {
        throw StubReviveContractApiError.unknownContract(contract)
    }

    func isAccountMapped(_: AccountId) async throws -> Bool { true }
}

enum StubReviveContractApiError: Error {
    case unknownContract(EvmAddress)
}
