package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import kotlinx.serialization.Serializable

/**
 * SCALE-serializable response for SignTransaction request.
 * Contains either just the signature or the full signed transaction.
 */
@Serializable
class SsoSignedPayloadJsonScale(
    val signature: ByteArray,
    val signedTx: ByteArray?,
)
