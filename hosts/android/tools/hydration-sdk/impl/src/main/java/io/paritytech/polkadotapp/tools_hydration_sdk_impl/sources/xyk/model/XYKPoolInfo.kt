package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import kotlinx.serialization.Serializable

@Serializable
@AsTuple
class XYKPoolInfo(val firstAsset: HydraDxAssetId, val secondAsset: HydraDxAssetId)
