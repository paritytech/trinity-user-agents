package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type.Orml.SubType
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

fun ExternalAssetId.Companion.fromHydrationAsset(chainAsset: Chain.Asset, hydrationAssetId: HydraDxAssetId): ExternalAssetId {
    return when (val type = chainAsset.type) {
        is Chain.Asset.Type.Native -> ExternalAssetId.Native
        is Chain.Asset.Type.Orml -> when (type.subType) {
            SubType.DEFAULT -> ExternalAssetId.Orml(AsRawScaleValue(hydrationAssetId))
            SubType.HYDRATION_EVM -> ExternalAssetId.HydrationEvm(AsRawScaleValue(hydrationAssetId))
        }

        else -> throw IllegalArgumentException("Unsupported asset type: ${chainAsset.type}")
    }
}
