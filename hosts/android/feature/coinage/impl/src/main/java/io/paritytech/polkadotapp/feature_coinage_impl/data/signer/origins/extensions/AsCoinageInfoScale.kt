@file:Keep

package io.paritytech.polkadotapp.feature_coinage_impl.data.signer.origins.extensions

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import kotlinx.serialization.Serializable

@Serializable
sealed class AsCoinageInfoScale : ToDynamicScaleInstance {
    override fun toEncodableInstance(): Any? = Scale.encode(this)

    @Serializable
    data object AsCoin : AsCoinageInfoScale()

    @Serializable
    @TransientStruct
    class AsUnloadTokenPeople(val body: Body) : AsCoinageInfoScale()

    @Serializable
    @TransientStruct
    class AsUnloadTokenLitePeople(val body: Body) : AsCoinageInfoScale()

    @Serializable
    class InfallibleUnpaidSigned(val nonce: BigIntegerSerializable) : AsCoinageInfoScale()

    @Serializable
    class Body(
        val proof: PeopleRingProof,
        val period: Long,
        val counter: Long,
        val aliasProofs: List<BandersnatchProof>,
    )
}

@Serializable
class PeopleRingProof(
    val proof: BandersnatchProof,
    val ring: RingIndex,
    val revision: RingRevision,
)
