package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId

interface FastLookupCustomFeeCapability {
    fun canPayFeeInNonUtilityToken(chainAssetId: ChainAssetId): Boolean
}
