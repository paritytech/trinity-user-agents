package io.paritytech.polkadotapp.feature_swap_impl.data.assetExchange

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_swap_api.domain.model.ReQuoteTrigger
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapDirection
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePaymentProvider
import kotlinx.coroutines.flow.Flow

interface AssetExchange {
    interface SingleChainFactory {
        suspend fun create(chain: Chain, swapHost: SwapHost): AssetExchange
    }

    interface MultiChainFactory {
        suspend fun create(swapHost: SwapHost): AssetExchange
    }

    interface SwapHost {
        val scope: ComputationalScope

        val sharedSubscriptions: SharedSwapSubscriptions

        suspend fun getFeePaymentProvider(): FeePaymentProvider

        suspend fun quote(quoteArgs: ParentQuoterArgs): Balance
    }

    suspend fun sync(): Result<Unit>

    suspend fun availableDirectSwapConnections(): List<SwapGraphEdge>

    suspend fun feePaymentOverrides(): List<FeePaymentProviderOverride>

    fun runSubscriptions(): Flow<ReQuoteTrigger>
}

data class FeePaymentProviderOverride(
    val provider: FeePaymentProvider,
    val chainId: ChainId
)

data class ParentQuoterArgs(
    val chainAssetIn: Chain.Asset,
    val chainAssetOut: Chain.Asset,
    val amount: Balance,
    val swapDirection: SwapDirection,
)
