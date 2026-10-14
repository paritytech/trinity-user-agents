package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.network.binding.bindEvent
import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.common.data.substrate.castToStruct

class XcmDryRunEffects(
    val executionResult: XcmOutcome,
    override val emittedEvents: List<GenericEvent.Instance>,
    override val forwardedXcms: ForwardedXcms
) : DryRunEffects {
    companion object {
        fun bind(decodedInstance: Any?): XcmDryRunEffects {
            val asStruct = decodedInstance.castToStruct()
            return XcmDryRunEffects(
                executionResult = XcmOutcome.bind(asStruct["executionResult"]),
                emittedEvents = bindList(asStruct["emittedEvents"], ::bindEvent),
                forwardedXcms = bindForwardedXcms(asStruct["forwardedXcms"])
            )
        }
    }
}
