package io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey

sealed interface DotNsLink {
    class LiteUsername(val label: String) : DotNsLink

    class None(val chatKey: EncodedPublicKey) : DotNsLink
}
