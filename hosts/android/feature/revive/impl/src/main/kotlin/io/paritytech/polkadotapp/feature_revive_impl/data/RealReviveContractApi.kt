package io.paritytech.polkadotapp.feature_revive_impl.data

import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.DynamicDispatchError
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.network.binding.mapError
import io.paritytech.polkadotapp.chains.network.binding.toDispatchError
import io.paritytech.polkadotapp.chains.network.binding.toResult
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.padEnd
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import io.paritytech.polkadotapp.feature_revive_api.ReviveDryRun
import io.paritytech.polkadotapp.feature_revive_api.toEvmAccountId
import java.math.BigInteger
import javax.inject.Inject

class RealReviveContractApi @Inject constructor(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
    @param:RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
) : ReviveContractApi {
    override suspend fun callReadOnly(
        chainId: ChainId,
        contract: EvmAccountId,
        input: DataByteArray,
        at: BlockHash?,
    ): Result<DataByteArray> {
        return runCatching {
            val runtimeCallsApi = multiChainRuntimeCallsApi.forChain(chainId)
            val contractResult = runtimeCallsApi.call<ReviveContractResult>(
                section = REVIVE_API,
                method = CALL_METHOD,
                arguments = callArguments(reviveOriginAccount(), contract, input),
                at = at,
            )

            runtimeCallsApi to contractResult.result
        }.flatMap { (runtimeCallsApi, result) -> result.toReturnValue(runtimeCallsApi) }
            .map { it.data }
    }

    override suspend fun dryRun(
        chainId: ChainId,
        origin: AccountId,
        contract: EvmAccountId,
        input: DataByteArray,
    ): Result<ReviveDryRun> {
        return runCatching {
            val runtimeCallsApi = multiChainRuntimeCallsApi.forChain(chainId)
            val dryRun = runtimeCallsApi.call<ReviveDryRunResult>(
                section = REVIVE_API,
                method = CALL_METHOD,
                arguments = callArguments(origin, contract, input),
            )

            runtimeCallsApi to dryRun
        }.flatMap { (runtimeCallsApi, dryRun) ->
            dryRun.result.toReturnValue(runtimeCallsApi).map { output ->
                ReviveDryRun(
                    data = output.data,
                    weightRequired = dryRun.weightRequired,
                    storageDeposit = dryRun.storageDeposit.charged(),
                )
            }
        }
    }

    override suspend fun isAccountMapped(chainId: ChainId, account: AccountId): Result<Boolean> {
        return remoteStorageSource.queryCatching(chainId) {
            metadata.revive.originalAccount.query(account.toEvmAccountId()) != null
        }
    }

    // The runtime is only needed to name a dispatch error, so it is not touched on the success path.
    private fun ScaleResult<ExecReturnValue, DynamicDispatchError>.toReturnValue(
        runtimeCallsApi: RuntimeCallsApi,
    ): Result<ExecReturnValue> {
        return mapError { it.toDispatchError(runtimeCallsApi.runtime) }
            .toResult()
            .flatMap { output ->
                if (output.isReverted) Result.failure(ReviveContractReverted(output.data)) else Result.success(output)
            }
    }

    private fun callArguments(origin: AccountId, contract: EvmAccountId, input: DataByteArray) = autoEncodedArgs(
        "origin" to origin,
        "dest" to contract,
        "value" to 0,
        "gas_limit" to WeightV2.max(),
        "storage_deposit_limit" to WeightV2.MAX_DIMENSION,
        "input_data" to input,
    )

    private fun ReviveStorageDeposit.charged(): BigInteger = when (this) {
        is ReviveStorageDeposit.Charge -> value
        is ReviveStorageDeposit.Refund -> BigInteger.ZERO
    }

    private fun reviveOriginAccount(): AccountId {
        return "modlpy/reviv".encodeToByteArray().padEnd(32).intoAccountId()
    }

    private companion object {
        const val REVIVE_API = "ReviveApi"
        const val CALL_METHOD = "call"
    }
}
