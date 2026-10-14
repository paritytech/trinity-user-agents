package io.paritytech.polkadotapp.tools_assethub_sdk_impl.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FastLookupCustomFeeCapability
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSwapEdge

class AssetConversionFastLookupFeeCapability(
    private val allowedPaymentAssets: Set<Int>,
) : FastLookupCustomFeeCapability {
    override fun canPayFeeInNonUtilityToken(chainAssetId: ChainAssetId): Boolean {
        return chainAssetId in allowedPaymentAssets
    }
}

fun AssetConversionFastLookupFeeCapability(
    availableSwapDirections: Collection<AssetHubSwapEdge>
): AssetConversionFastLookupFeeCapability {
    val availableFeeAssets = availableSwapDirections.mapToSet { it.toAsset.id }
    return AssetConversionFastLookupFeeCapability(availableFeeAssets)
}
