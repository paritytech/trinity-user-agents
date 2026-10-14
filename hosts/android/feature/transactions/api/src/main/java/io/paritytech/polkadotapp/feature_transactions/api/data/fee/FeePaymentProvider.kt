package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

interface FeePaymentProvider {
    suspend fun getFeePayment(feeAsset: Chain.Asset): FeePayment?

    suspend fun feeCapabilityLookup(chainId: ChainId): Result<FastLookupCustomFeeCapability>
}

suspend fun FeePaymentProvider.getFeePaymentOrNative(feeAsset: Chain.Asset): FeePayment {
    return getFeePayment(feeAsset) ?: NativeFeePayment()
}
