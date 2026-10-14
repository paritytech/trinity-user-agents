package io.paritytech.polkadotapp.feature_chain_resources_api.data.model

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.Serializable

@Keep
@Serializable
sealed class OnChainStmtStoreSlot {
    @Keep
    @Serializable
    class Occupied(
        val accountId: DataByteArray,
        val since: BigIntegerSerializable,
    ) : OnChainStmtStoreSlot()

    @Keep
    @Serializable
    object Free : OnChainStmtStoreSlot()
}
