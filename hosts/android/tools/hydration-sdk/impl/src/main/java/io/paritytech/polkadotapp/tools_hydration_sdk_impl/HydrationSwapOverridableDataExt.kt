package io.paritytech.polkadotapp.tools_hydration_sdk_impl

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalBalanceTypeSubscriptions
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import kotlinx.coroutines.flow.Flow

fun HydrationSwapOverridableData.toExternalBalanceTypeSubscriptions(): ExternalBalanceTypeSubscriptions {
    return object : ExternalBalanceTypeSubscriptions {
        override suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber> {
            return this@toExternalBalanceTypeSubscriptions.blockNumber(chainId)
        }
    }
}
