package io.paritytech.polkadotapp.feature_xcm_api.extrinsic

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.xcmPalletName
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.versions.toEncodableInstance

context(withRuntime: WithRuntime)
fun composeXcmExecute(
    message: VersionedXcmMessage,
    maxWeight: WeightV2,
): GenericCall.Instance {
    return composeCall(
        moduleName = withRuntime.runtime.metadata.xcmPalletName(),
        callName = "execute",
        arguments = mapOf(
            "message" to message.toEncodableInstance(),
            "max_weight" to maxWeight.toEncodableInstance()
        )
    )
}
