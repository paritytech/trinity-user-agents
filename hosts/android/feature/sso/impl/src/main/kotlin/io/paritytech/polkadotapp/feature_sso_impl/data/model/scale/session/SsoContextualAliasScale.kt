package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import kotlinx.serialization.Serializable

@Serializable
class SsoContextualAliasScale(
    @FixedLength(32)
    val context: ByteArray,
    val alias: ByteArray,
)
