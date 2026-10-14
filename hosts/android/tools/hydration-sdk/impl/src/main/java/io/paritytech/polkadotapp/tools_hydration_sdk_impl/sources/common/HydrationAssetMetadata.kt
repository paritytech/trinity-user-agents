package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.paritytech.polkadotapp.feature_balances_api.data.type.external.ExternalAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

class HydrationAssetMetadata(
    val assetId: HydraDxAssetId,
    val decimals: Int,
    val assetType: String
) {
    fun determineExternalAssetId(nativeId: HydraDxAssetId): ExternalAssetId {
        return when {
            assetId == nativeId -> ExternalAssetId.Native
            assetType == "Erc20" -> ExternalAssetId.HydrationEvm(AsRawScaleValue(assetId))
            else -> ExternalAssetId.Orml(AsRawScaleValue(assetId))
        }
    }
}

class HydrationAssetMetadataMap(
    private val nativeId: HydraDxAssetId,
    private val metadataMap: Map<HydraDxAssetId, HydrationAssetMetadata>
) {
    fun getAssetType(assetId: HydraDxAssetId): ExternalAssetId? {
        val metadata = metadataMap[assetId] ?: return null

        return metadata.determineExternalAssetId(nativeId)
    }

    fun getDecimals(assetId: HydraDxAssetId): Int? {
        val metadata = metadataMap[assetId] ?: return null

        return metadata.decimals
    }
}
