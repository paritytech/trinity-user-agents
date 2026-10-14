package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model

import io.paritytech.polkadotapp.chains.network.binding.DispatchError
import io.paritytech.polkadotapp.chains.network.binding.bindDispatchError
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.data.substrate.castToStruct

class DispatchErrorWithPostInfo(
    val postInfo: DispatchPostInfo,
    val error: DispatchError
) {
    companion object {
        context(withRuntime: WithRuntime)
        fun bind(decodedInstance: Any?): DispatchErrorWithPostInfo {
            val asStruct = decodedInstance.castToStruct()

            return DispatchErrorWithPostInfo(
                postInfo = DispatchPostInfo.bind(asStruct["post_info"]),
                error = bindDispatchError(asStruct["error"])
            )
        }
    }
}
