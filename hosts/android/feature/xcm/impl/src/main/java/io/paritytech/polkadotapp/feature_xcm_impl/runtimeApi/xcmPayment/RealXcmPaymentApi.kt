package io.paritytech.polkadotapp.feature_xcm_impl.runtimeApi.xcmPayment

import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment.XcmPaymentApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.xcmPayment.model.QueryXcmWeightErr
import io.paritytech.polkadotapp.feature_xcm_api.versions.toEncodableInstance
import javax.inject.Inject

class RealXcmPaymentApi @Inject constructor(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi,
) : XcmPaymentApi {
    override suspend fun queryXcmWeight(
        chainId: ChainId,
        xcm: VersionedXcmMessage
    ): Result<ScaleResult<WeightV2, QueryXcmWeightErr>> {
        return multiChainRuntimeCallsApi.forChain(chainId).queryXcmWeight(xcm)
    }

    override suspend fun isSupported(chainId: ChainId): Boolean {
        return multiChainRuntimeCallsApi.isSupported(chainId, "XcmPaymentApi", "query_xcm_weight")
    }

    private suspend fun RuntimeCallsApi.queryXcmWeight(
        xcm: VersionedXcmMessage,
    ): Result<ScaleResult<WeightV2, QueryXcmWeightErr>> {
        return runCatching {
            call(
                section = "XcmPaymentApi",
                method = "query_xcm_weight",
                arguments = mapOf(
                    "message" to xcm.toEncodableInstance()
                ),
                returnBinding = {
                    ScaleResult.bind(
                        dynamicInstance = it,
                        bindOk = WeightV2::bind,
                        bindError = QueryXcmWeightErr::bind
                    )
                }
            )
        }
    }
}
