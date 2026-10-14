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

/// Registered installations per (contract, block hash); a read of an unlisted pair fails.
final class StubDataStoreRepository: AccountDataStoreRepositoryProtocol, @unchecked Sendable {
    struct Key: Hashable {
        let contract: EvmAddress
        let blockHash: Data?
    }

    private let lists = OSAllocatedUnfairLock<[Key: Result<Set<CoinageInstallationId>, Error>]>(initialState: [:])
    private let readCount = OSAllocatedUnfairLock(initialState: 0)
    private let transientFailures = OSAllocatedUnfairLock(initialState: 0)
    private let registrationInput = OSAllocatedUnfairLock<Data>(initialState: Data([0xE5, 0x61, 0x86, 0x8D]))
    var account = DataStoreAccount(
        privateKey: Data(repeating: 0x0A, count: 64),
        publicKey: Data(repeating: 0x0A, count: 32),
        evmAccountId: EvmAddress(repeating: 0x0E, count: EvmAddressFormat.size),
        encryptionKey: Data(repeating: 0, count: 32)
    )

    var reads: Int { readCount.withLock { $0 } }

    func listed(
        _ installations: Set<CoinageInstallationId>,
        contract: EvmAddress = TestContracts.contract,
        at blockHash: Data? = nil
    ) {
        lists.withLock { $0[Key(contract: contract, blockHash: blockHash)] = .success(installations) }
    }

    func failing(
        contract: EvmAddress = TestContracts.contract,
        at blockHash: Data? = nil,
        error: Error = InstallationStubError.unreachable
    ) {
        lists.withLock { $0[Key(contract: contract, blockHash: blockHash)] = .failure(error) }
    }

    /// The next `count` reads fail before the listing answers, whatever it holds.
    func failingTransiently(times count: Int) {
        transientFailures.withLock { $0 = count }
    }

    func fetchRegisteredInstallations(
        contract: EvmAddress,
        at blockHash: Data?
    ) async throws -> Set<CoinageInstallationId> {
        readCount.withLock { $0 += 1 }
        let failsThisRead = transientFailures.withLock { remaining in
            guard remaining > 0 else { return false }
            remaining -= 1
            return true
        }
        if failsThisRead {
            throw InstallationStubError.unreachable
        }
        guard let result = lists.withLock({ $0[Key(contract: contract, blockHash: blockHash)] }) else {
            throw InstallationStubError.unreachable
        }
        return try result.get()
    }

    func registrationCall(target: InstallationRegistrationTarget) async throws -> InstallationRegistrationCall {
        InstallationRegistrationCall(
            account: account,
            contract: target.contract,
            input: registrationInput.withLock { $0 }
        )
    }
}
