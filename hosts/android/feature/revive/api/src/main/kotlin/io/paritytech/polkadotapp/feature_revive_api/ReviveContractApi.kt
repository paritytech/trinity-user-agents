package io.paritytech.polkadotapp.feature_revive_api

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import org.bouncycastle.jcajce.provider.digest.Keccak
import java.math.BigInteger

typealias EvmAccountId = DataByteArray

/** What a contract call would do if [origin] sent it now: its output and the limits a real call must declare. */
data class ReviveDryRun(
    val data: DataByteArray,
    val weightRequired: WeightV2,
    /** The deposit the call would charge; zero when it would only refund. */
    val storageDeposit: BigInteger,
)

/** The contract ran and reverted; [data] is what it returned as the reason. */
class ReviveContractReverted(val data: DataByteArray) : Exception("Contract call reverted")

interface ReviveContractApi {
    /**
     * Evaluates a read-only call against the state at [at], or at the node's latest block when null.
     * A revert fails with [ReviveContractReverted].
     */
    suspend fun callReadOnly(
        chainId: ChainId,
        contract: EvmAccountId,
        input: DataByteArray,
        at: BlockHash? = null,
    ): Result<DataByteArray>

    /**
     * Simulates [input] as [origin] would send it. A revert fails with [ReviveContractReverted].
     *
     * The origin matters: limits simulated from any other account describe a different call.
     */
    suspend fun dryRun(
        chainId: ChainId,
        origin: AccountId,
        contract: EvmAccountId,
        input: DataByteArray,
    ): Result<ReviveDryRun>

    /**
     * Whether [account] has an H160 the pallet recognises as its own. An unmapped origin is rejected by every
     * contract call, and a dry-run cannot tell: the pallet maps the origin for the length of the simulation.
     */
    suspend fun isAccountMapped(chainId: ChainId, account: AccountId): Result<Boolean>
}

/**
 * Maps a Substrate AccountId32 to its pallet-revive H160 AccountId.
 *
 * pallet-revive's default `AccountIdMapper` derives the EVM AccountId as
 * the last 20 bytes of `keccak256(accountId)` (see runtime `pallet-revive`).
 */
fun AccountId.toEvmAccountId(): EvmAccountId {
    val keccak = Keccak.Digest256().digest(value)
    return keccak.copyOfRange(12, 32).toDataByteArray()
}
