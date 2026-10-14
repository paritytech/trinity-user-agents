package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.HydrationSwapEdge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import kotlinx.coroutines.flow.Flow

interface HydrationSwapSource {
    suspend fun sync()

    suspend fun availableSwapDirections(): Collection<HydrationSwapEdge>

    suspend fun runSubscriptions(
        subscriptionBuilder: SharedRequestsBuilder
    ): Flow<Unit>

    interface Factory {
        fun create(
            chain: Chain,
            overridableData: HydrationSwapOverridableData,
            weightSpec: WeightSpec,
        ): HydrationSwapSource
    }
}
