package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.extension

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_dotns_gateway_api.domain.model.DotNsLink
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model.DotNsLinkScale
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.model.toScale
import kotlinx.serialization.Serializable

@Serializable
class DotNsRegisterProofMessage(
    @FixedLength(32) val who: ByteArraySerializable,
    val label: ByteArraySerializable,
    val link: DotNsLinkScale
) {
    companion object {
        fun hash(who: AccountId, label: String, link: DotNsLink): ByteArray {
            return DotNsRegisterProofMessage(
                who = who.value,
                label = label.encodeToByteArray(),
                link = link.toScale()
            ).scaleEncodeBinary().blake2b256()
        }
    }
}
