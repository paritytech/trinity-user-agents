package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

@JvmInline
@Serializable
value class WeightV1(val refTime: BigIntegerSerializable)
