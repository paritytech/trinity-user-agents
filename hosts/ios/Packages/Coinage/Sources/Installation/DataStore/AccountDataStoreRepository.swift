import Foundation
import Revive
import SDKLogger
import SubstrateSdk

public struct InstallationRegistrationCall: Sendable {
    public let account: DataStoreAccount
    public let contract: EvmAddress
    public let input: Data
}

public protocol AccountDataStoreRepositoryProtocol: Sendable {
    /// Records this seed cannot open are skipped rather than failing the read: the list belongs to the
    /// account, and nothing guarantees every entry in it was written by this app.
    func fetchRegisteredInstallations(
        contract: EvmAddress,
        at blockHash: Data?
    ) async throws -> Set<CoinageInstallationId>

    func registrationCall(target: InstallationRegistrationTarget) async throws -> InstallationRegistrationCall
}

final class AccountDataStoreRepository: AccountDataStoreRepositoryProtocol {
    private let accountKeys: any DataStoreAccountKeysProviding
    private let reviveApi: any ReviveContractApiProtocol
    private let logger: (any SDKLoggerProtocol)?

    init(
        accountKeys: any DataStoreAccountKeysProviding,
        reviveApi: any ReviveContractApiProtocol,
        logger: (any SDKLoggerProtocol)?
    ) {
        self.accountKeys = accountKeys
        self.reviveApi = reviveApi
        self.logger = logger
    }

    func fetchRegisteredInstallations(
        contract: EvmAddress,
        at blockHash: Data?
    ) async throws -> Set<CoinageInstallationId> {
        let account = try await accountKeys.account()
        let input = try AccountDataStoreAbi.encodeGetInstallations(owner: account.evmAccountId)
        let output = try await reviveApi.callReadOnly(contract: contract, input: input, at: blockHash)
        let records = try AccountDataStoreAbi.decodeGetInstallations(output: output)

        let opened = Set(records.compactMap { InstallationRecordCipher.open($0, key: account.encryptionKey) })
        if opened.count < records.count {
            let skipped = records.count - opened.count
            logger?.warning("Skipped \(skipped) of \(records.count) data store records this seed cannot open")
        }
        return opened
    }

    func registrationCall(target: InstallationRegistrationTarget) async throws -> InstallationRegistrationCall {
        let account = try await accountKeys.account()
        let record = try InstallationRecordCipher.seal(target.installation, key: account.encryptionKey)

        return try InstallationRegistrationCall(
            account: account,
            contract: target.contract,
            input: AccountDataStoreAbi.encodeRegisterInstallation(record: record)
        )
    }
}
