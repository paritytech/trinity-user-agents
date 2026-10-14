import ChainStore
import Foundation
import Operation_iOS
import SubstrateSdk

/// ``ReviveContractApiProtocol`` bound to one chain: reads run as ``ReviveReadOnlyCaller`` unless told
/// otherwise, a reverted answer is an error, and a dry run reports the limits a real call must declare.
public final class ReviveContractApi: ReviveContractApiProtocol, @unchecked Sendable {
    private let chainId: ChainId
    private let caller: any ReviveContractCalling
    private let readOnlyOrigin: AccountId

    public convenience init(
        chainId: ChainId,
        chainResource: ChainResourceProtocol,
        operationQueue: OperationQueue,
        readOnlyOrigin: AccountId = ReviveReadOnlyCaller.accountId
    ) {
        self.init(
            chainId: chainId,
            caller: ReviveContractCaller(chainResource: chainResource, operationQueue: operationQueue),
            readOnlyOrigin: readOnlyOrigin
        )
    }

    init(chainId: ChainId, caller: any ReviveContractCalling, readOnlyOrigin: AccountId) {
        self.chainId = chainId
        self.caller = caller
        self.readOnlyOrigin = readOnlyOrigin
    }

    public func callReadOnly(contract: EvmAddress, input: Data, at blockHash: Data?) async throws -> Data {
        let result = try await caller.call(
            chainId: chainId,
            origin: readOnlyOrigin,
            contract: contract,
            input: input,
            at: blockHash?.toHex(includePrefix: true)
        )

        return try Self.completedOutput(of: result).output
    }

    public func dryRun(origin: AccountId, contract: EvmAddress, input: Data) async throws -> ReviveDryRun {
        let result = try await caller.call(chainId: chainId, origin: origin, contract: contract, input: input, at: nil)
        let output = try Self.completedOutput(of: result)

        return ReviveDryRun(
            data: output.output,
            weightRequired: result.weightRequired,
            storageDeposit: result.storageDeposit.charged
        )
    }

    public func isAccountMapped(_ account: AccountId) async throws -> Bool {
        try await caller.isAccountMapped(chainId: chainId, account: account)
    }
}

private extension ReviveContractApi {
    static func completedOutput(of result: ReviveDryRunResult) throws -> ReviveExecResult {
        let execResult = try result.result.ensureOkOrError { ReviveContractError.callFailed($0) }

        guard !execResult.isReverted else {
            throw ReviveContractRevertedError(data: execResult.output)
        }

        return execResult
    }
}
