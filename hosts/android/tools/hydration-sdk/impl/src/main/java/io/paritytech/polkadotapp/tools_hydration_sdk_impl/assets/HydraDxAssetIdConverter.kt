package io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain

typealias HydraDxAssetId = BigIntegerSerializable
typealias HydraRemoteToLocalMapping = Map<HydraDxAssetId, Chain.Asset>

interface HydraDxAssetIdConverter {
    val systemAssetId: HydraDxAssetId

    suspend fun toOnChainIdOrNull(chainAsset: Chain.Asset): HydraDxAssetId?

    suspend fun toChainAssetOrNull(chain: Chain, onChainId: HydraDxAssetId): Chain.Asset?

    suspend fun allOnChainIds(chain: Chain): HydraRemoteToLocalMapping
}

fun HydraDxAssetIdConverter.isSystemAsset(assetId: HydraDxAssetId): Boolean {
    return assetId == systemAssetId
}

suspend fun HydraDxAssetIdConverter.toOnChainIdOrThrow(chainAsset: Chain.Asset): HydraDxAssetId {
    return requireNotNull(toOnChainIdOrNull(chainAsset))
}

suspend fun HydraDxAssetIdConverter.toChainAssetOrThrow(chain: Chain, onChainId: HydraDxAssetId): Chain.Asset {
    return requireNotNull(toChainAssetOrNull(chain, onChainId))
}
