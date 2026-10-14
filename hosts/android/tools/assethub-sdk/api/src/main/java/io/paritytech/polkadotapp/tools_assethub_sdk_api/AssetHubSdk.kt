package io.paritytech.polkadotapp.tools_assethub_sdk_api

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePaymentProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSdkOverridableData
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSwapEdge
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.AssetHubDryRunOutcome
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapLimit
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapOutcome
import kotlinx.coroutines.flow.Flow

interface AssetHubSdk : FeePaymentProvider {
    interface Factory {
        suspend fun create(
            chainId: ChainId,
            overridableData: AssetHubSdkOverridableData? = null
        ): AssetHubSdk
    }

    val chain: Chain

    suspend fun availableSwapDirections(): Collection<AssetHubSwapEdge>

    fun quoteInvalidationFlow(): Flow<Unit>

    suspend fun performSwap(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
        origin: TransactionOrigin,
        recipient: AccountId
    ): Result<SwapOutcome>

    suspend fun dryRun(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        origin: TransactionOrigin,
        recipient: AccountId
    ): Result<AssetHubDryRunOutcome>

    suspend fun estimateFee(
        trade: AssetHubSwapEdge,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
    ): Result<AccountFee>
}
