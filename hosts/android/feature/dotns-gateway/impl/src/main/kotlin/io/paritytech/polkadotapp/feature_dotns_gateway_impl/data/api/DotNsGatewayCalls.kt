package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.api

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model.DotNsLinkScale
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model.toScale

@JvmInline
value class DotNsGatewayCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.dotNsGatewayCalls: DotNsGatewayCalls
    get() = DotNsGatewayCalls(this)

fun DotNsGatewayCalls.registerName(
    who: AccountId,
    label: String,
    link: DotNsLink
) {
    extrinsicBuilder.call(
        moduleName = Modules.DOTNS_GATEWAY,
        callName = "register_name",
        arguments = autoEncodedArgs<_, _, DotNsLinkScale>(
            "who" to who,
            "label" to label,
            "link" to link.toScale()
        )
    )
}
