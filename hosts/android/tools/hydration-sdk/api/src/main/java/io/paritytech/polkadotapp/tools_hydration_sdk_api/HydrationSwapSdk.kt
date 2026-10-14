package io.paritytech.polkadotapp.tools_hydration_sdk_api

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FeePaymentProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.HydrationSwapDryRunOutcome
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapLimit
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapOutcome
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow

interface HydrationSwapSdk : FeePaymentProvider {
    interface Factory {
        companion object {
            private const val DEFAULT_BASE_WEIGHT = 100
        }

        suspend fun create(
            chain: Chain,
            coroutineScope: CoroutineScope,
            weightSpec: WeightSpec = WeightSpec.fromBaseWeight(DEFAULT_BASE_WEIGHT),
            overridableData: HydrationSwapOverridableData? = null
        ): HydrationSwapSdk
    }

    val chain: Chain

    suspend fun sync(): Result<Unit>

    suspend fun availableSwapDirections(): Collection<HydrationSwapEdge>

    fun runSubscriptions(): Flow<Unit>

    suspend fun submit(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
        origin: TransactionOrigin,
    ): Result<SwapOutcome>

    suspend fun dryRun(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        origin: TransactionOrigin,
    ): Result<HydrationSwapDryRunOutcome>

    suspend fun estimateFee(
        trade: Path<HydrationSwapEdge>,
        swapLimit: SwapLimit,
        feeAsset: Chain.Asset,
    ): Result<AccountFee>
}
