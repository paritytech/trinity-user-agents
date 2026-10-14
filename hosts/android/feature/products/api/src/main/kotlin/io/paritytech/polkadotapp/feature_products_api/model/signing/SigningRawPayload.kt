package io.paritytech.polkadotapp.feature_products_api.model.signing

import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

class SigningRawPayload(
    val account: ProductAccountId,
    val type: RawPayloadContent,
)

sealed interface RawPayloadContent {
    class Bytes(val data: ByteArray) : RawPayloadContent
    class Payload(val data: String) : RawPayloadContent
}
