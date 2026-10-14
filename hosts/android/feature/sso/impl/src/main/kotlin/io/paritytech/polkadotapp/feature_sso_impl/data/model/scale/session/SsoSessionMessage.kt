package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
class SsoSessionMessage(
    val id: String,
    val versioned: VersionedSsoSessionMessage
)

@Serializable
sealed interface VersionedSsoSessionMessage {
    @Serializable
    @EnumIndex(0)
    class V1(val message: SsoSessionMessageV1) : VersionedSsoSessionMessage
}

@Serializable
class SsoSessionMessageV1(
    val content: SsoMessageContent
)

@Serializable
class AlwaysDecodableSsoMessagePart(
    val id: String,
)
