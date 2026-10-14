import Foundation
import SubstrateSdk

/// The chain side of ``ReviveContractApi``: one runtime-API simulation and one storage read, without
/// interpreting the answer.
protocol ReviveContractCalling: Sendable {
    /// `ReviveApi_call` as `origin` against the state at `blockHash`, or the latest block when nil.
    func call(
        chainId: ChainId,
        origin: AccountId,
        contract: EvmAddress,
        input: Data,
        at blockHash: BlockHash?
    ) async throws -> ReviveDryRunResult

    /// Whether `Revive.OriginalAccount` holds an entry for the H160 of `account`.
    func isAccountMapped(chainId: ChainId, account: AccountId) async throws -> Bool
}
