package io.paritytech.polkadotapp.feature_sso_impl.domain.session.model

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey

@JvmInline
value class SsoSessionId(val value: String) {
    fun toRawSharedSecretPublicKey(): ByteArray {
        return value.fromHex()
    }

    companion object {
        fun fromSharedPubKey(sharedPublicKey: EncodedPublicKey): SsoSessionId {
            return SsoSessionId(sharedPublicKey.value.toHexString())
        }
    }
}
