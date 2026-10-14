@file:Keep

package io.paritytech.polkadotapp.feature_pgas_impl.data.extension

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import kotlinx.serialization.Serializable

@Serializable
sealed class AsPgasInfoScale : ToDynamicScaleInstance {
    override fun toEncodableInstance(): Any? = Scale.encode(this)

    @Serializable
    class Claim(
        val proof: BandersnatchProof,
        val ringIndex: RingIndex,
        val revision: RingRevision,
        val collection: PgasCollectionScale,
        val day: UInt,
    ) : AsPgasInfoScale()
}

@Serializable
sealed interface PgasCollectionScale {
    @Serializable
    object People : PgasCollectionScale

    @Serializable
    object LitePeople : PgasCollectionScale
}
