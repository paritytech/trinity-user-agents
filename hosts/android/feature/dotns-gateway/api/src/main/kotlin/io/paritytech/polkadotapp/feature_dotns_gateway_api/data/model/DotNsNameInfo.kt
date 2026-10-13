package io.paritytech.polkadotapp.feature_dotns_gateway_api.data.model

import androidx.annotation.Keep
import io.paritytech.polkadotapp.common.utils.scale.AccountEcdhKeyScale
import kotlinx.serialization.Serializable

@Keep
@Serializable
class DotNsOnChainConsumerInfo(
    val lite: DotNsUsername?,
    val full: DotNsUsername?
)


@Keep
@Serializable
class DotNsUsername(
    val label: String,
    val chat: AccountEcdhKeyScale?
)
