@file:Keep

package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.extension

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchProof
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_members_api.data.model.RingIndex
import io.paritytech.polkadotapp.feature_members_api.data.model.RingRevision
import kotlinx.serialization.Serializable

@Serializable
sealed class AsResourcesInfoScale : ToDynamicScaleInstance {
    override fun toEncodableInstance(): Any? = Scale.encode(this)

    @Serializable
    @AsTuple
    class ClaimLongTermStorage(
        val proof: BandersnatchProof,
        val ringIndex: RingIndex,
        val revision: RingRevision,
        val collection: MembershipCollectionScale,
    ) : AsResourcesInfoScale()
}

@Serializable
sealed interface MembershipCollectionScale {
    @Serializable
    object People : MembershipCollectionScale

    @Serializable
    object LitePeople : MembershipCollectionScale
}
