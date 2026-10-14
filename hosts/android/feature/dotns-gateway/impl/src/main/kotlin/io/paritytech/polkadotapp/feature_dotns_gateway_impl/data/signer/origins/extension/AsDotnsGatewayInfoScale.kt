@file:Keep

package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.extension

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.common.data.substrate.model.MultiSignature
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import kotlinx.serialization.Serializable

@Serializable
sealed class AsDotnsGatewayInfoScale : ToDynamicScaleInstance {
    override fun toEncodableInstance(): Any? = Scale.encode(this)

    @Serializable
    class RegisterFullName(
        val proof: BandersnatchProof,
        val ringIndex: RingIndex,
        val revision: RingRevision,
        val signature: MultiSignature
    ) : AsDotnsGatewayInfoScale()
}
