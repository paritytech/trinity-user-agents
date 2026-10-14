package io.paritytech.polkadotapp.feature_tokens_api.presentation.model

import android.os.Parcelable
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import kotlinx.parcelize.Parcelize

@Parcelize
class AssetPayload(val chainId: ChainId, val chainAssetId: Int) : Parcelable

fun AssetPayload.toFullChainAssetId() = FullChainAssetId(chainId, chainAssetId)

fun FullChainAssetId.toAssetPayload() = AssetPayload(chainId, assetId)
