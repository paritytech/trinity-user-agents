package io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.paritytech.polkadotapp.chains.call.ViewFunctionsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.noArgs
import io.paritytech.polkadotapp.chains.util.Modules
import kotlinx.serialization.Serializable

@Serializable
@AsTuple
class FreeUnloadTokenInfo(val peopleLimit: Long, val litePeopleLimit: Long)

suspend fun ViewFunctionsApi.getFreeUnloadTokenInfo(): Result<FreeUnloadTokenInfo> {
    return call(
        pallet = Modules.COINAGE,
        name = "get_free_unload_token_info",
        arguments = noArgs()
    )
}

suspend fun ViewFunctionsApi.getMaximumAge(): Result<UShort> {
    return call(
        pallet = Modules.COINAGE,
        name = "get_maximum_age",
        arguments = noArgs()
    )
}
