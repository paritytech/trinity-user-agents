package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.bindEvent
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.message.bindVersionedRawXcmMessage

class CallDryRunEffects(
    val executionResult: ScaleResult<DispatchPostInfo, DispatchErrorWithPostInfo>,
    override val emittedEvents: List<GenericEvent.Instance>,
    // We don't need to fully decode/encode intermediate xcm messages
    val localXcm: VersionedRawXcmMessage?,
    override val forwardedXcms: ForwardedXcms
) : DryRunEffects {
    companion object {
        context(withRuntime: WithRuntime)
        fun bind(decodedInstance: Any?): CallDryRunEffects {
            val asStruct = decodedInstance.castToStruct()
            return CallDryRunEffects(
                executionResult = ScaleResult.bind(
                    dynamicInstance = asStruct["executionResult"],
                    bindOk = DispatchPostInfo::bind,
                    bindError = { DispatchErrorWithPostInfo.bind(it) }
                ),
                emittedEvents = bindList(asStruct["emittedEvents"], ::bindEvent),
                localXcm = asStruct.get<Any?>("localXcm")?.let { bindVersionedRawXcmMessage(it) },
                forwardedXcms = bindForwardedXcms(asStruct["forwardedXcms"])
            )
        }
    }
}
