package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.toResult

class FeePaymentRegistry(
    private val providersByChain: Map<String, FeePaymentProvider>
) : FeePaymentProvider {
    override suspend fun getFeePayment(feeAsset: Chain.Asset): FeePayment? {
        return providersByChain[feeAsset.chainId]?.getFeePayment(feeAsset)
    }

    override suspend fun feeCapabilityLookup(chainId: ChainId): Result<FastLookupCustomFeeCapability> {
        return providersByChain[chainId].toResult()
            .flatMap { it.feeCapabilityLookup(chainId) }
    }
}
