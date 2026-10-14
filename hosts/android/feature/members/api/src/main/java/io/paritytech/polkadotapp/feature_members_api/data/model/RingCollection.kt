package io.paritytech.polkadotapp.feature_members_api.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

@Serializable
class RingCollection(
    val ringSize: RingExponent,
    val selfInclusionDelay: BigIntegerSerializable?
)
