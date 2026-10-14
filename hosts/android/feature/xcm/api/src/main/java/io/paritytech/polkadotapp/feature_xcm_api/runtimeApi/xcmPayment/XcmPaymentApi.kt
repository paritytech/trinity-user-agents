package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment.model.QueryXcmWeightErr

interface XcmPaymentApi {
    suspend fun queryXcmWeight(
        chainId: ChainId,
        xcm: VersionedXcmMessage,
    ): Result<ScaleResult<WeightV2, QueryXcmWeightErr>>

    suspend fun isSupported(chainId: ChainId): Boolean
}
