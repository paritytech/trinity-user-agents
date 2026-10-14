package io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import kotlinx.serialization.Serializable

@Serializable
@AsTuple
data class Tuple2<A, B>(val first: A, val second: B)
