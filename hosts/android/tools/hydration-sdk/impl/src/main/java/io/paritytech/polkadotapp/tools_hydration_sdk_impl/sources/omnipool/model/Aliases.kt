package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraRemoteToLocalMapping

typealias RemoteAndLocalId = Pair<HydraDxAssetId, FullChainAssetId>
typealias RemoteIdAndLocalAsset = Pair<HydraDxAssetId, Chain.Asset>
typealias RemoteAndLocalIdOptional = Pair<HydraDxAssetId, FullChainAssetId?>

@Suppress("UNCHECKED_CAST")
fun RemoteAndLocalIdOptional.flatten(): RemoteAndLocalId? {
    return second?.let { this as RemoteAndLocalId }
}

val RemoteAndLocalId.remoteId
    get() = first

val RemoteAndLocalId.localId
    get() = second

fun HydraRemoteToLocalMapping.matchId(remoteId: HydraDxAssetId): RemoteAndLocalId? {
    return get(remoteId)?.fullId?.let { remoteId to it }
}
