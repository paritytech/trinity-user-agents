package io.paritytech.polkadotapp.feature_balances_impl.data.type.hydrationEvm

import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.UntypedOrmlCurrencyId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalBalanceTypeSubscriptions
import io.paritytech.polkadotapp.feature_balances_impl.data.type.orml.model.OrmlAssetAccount
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class HydrationEvmBalancePoller @Inject constructor(
    private val runtimeCallsApi: MultiChainRuntimeCallsApi,
    private val chainStateRepository: ChainStateRepository,
    private val rpcCalls: RpcCalls,
) {
    suspend fun pollBalanceFlow(
        chainId: ChainId,
        accountId: AccountId,
        assetId: UntypedOrmlCurrencyId,
        subscriptionBuilder: SharedRequestsBuilder? = null,
        externalBalanceTypeSubscriptions: ExternalBalanceTypeSubscriptions? = null
    ): Flow<HydrationEvmBalancePollUpdate> {
        val blockNumberFlow = externalBalanceTypeSubscriptions?.blockNumber(chainId)
            ?: chainStateRepository.currentRemoteBlockNumberFlow(chainId, subscriptionBuilder)

        return flow {
            val initialBalance = fetchBalance(chainId, accountId, assetId)
            emit(HydrationEvmBalancePollUpdate(initialBalance, at = null))

            var currentBalance = initialBalance

            blockNumberFlow.collect { blockNumber ->
                val newBalance = fetchBalance(chainId, accountId, assetId)

                if (currentBalance != newBalance) {
                    currentBalance = newBalance

                    val balanceUpdatedAt = rpcCalls.getBlockHash(chainId, blockNumber)
                    emit(HydrationEvmBalancePollUpdate(newBalance, balanceUpdatedAt))
                }
            }
        }
    }

    suspend fun fetchBalance(
        chainId: ChainId,
        accountId: AccountId,
        assetId: UntypedOrmlCurrencyId,
    ): OrmlAssetAccount {
        return runtimeCallsApi.forChain(chainId).fetchBalance(accountId, assetId)
    }

    private suspend fun RuntimeCallsApi.fetchBalance(
        accountId: AccountId,
        assetId: UntypedOrmlCurrencyId,
    ): OrmlAssetAccount {
        return call(
            section = "CurrenciesApi",
            method = "account",
            arguments = autoEncodedArgs(
                "asset_id" to assetId,
                "who" to accountId
            ),
        )
    }
}
