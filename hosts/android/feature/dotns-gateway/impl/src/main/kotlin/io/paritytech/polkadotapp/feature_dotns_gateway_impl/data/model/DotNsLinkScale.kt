package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import kotlinx.serialization.Serializable

@Serializable
sealed class DotNsLinkScale {
    @Serializable
    @TransientStruct
    @EnumIndex(0)
    class LiteUsername(val label: String) : DotNsLinkScale()

    @Serializable
    @TransientStruct
    @EnumIndex(1)
    class None(@FixedLength(65) val chatKey: ByteArraySerializable) : DotNsLinkScale()
}

fun DotNsLink.toScale(): DotNsLinkScale = when (this) {
    is DotNsLink.LiteUsername -> DotNsLinkScale.LiteUsername(label)
    is DotNsLink.None -> DotNsLinkScale.None(chatKey.value)
}
