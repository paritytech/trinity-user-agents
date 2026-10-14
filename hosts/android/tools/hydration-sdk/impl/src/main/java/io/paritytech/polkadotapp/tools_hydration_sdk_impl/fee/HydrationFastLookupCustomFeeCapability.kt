package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.common.utils.mapNotNullToSet
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.FastLookupCustomFeeCapability
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.api.acceptedCurrencies
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.api.multiTransactionPayment
import javax.inject.Inject

class HydrationFastLookupCustomFeeCapabilityFactory @Inject constructor(
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val idConverter: HydraDxAssetIdConverter,
) {
    suspend fun create(chain: Chain): Result<HydrationFastLookupCustomFeeCapability> {
        return remoteStorageSource.queryCatching(chain.id) {
            val acceptedOnChainIds = metadata.multiTransactionPayment.acceptedCurrencies.keys()
            val onChainToLocalIds = idConverter.allOnChainIds(chain)

            acceptedOnChainIds.mapNotNullToSet { onChainToLocalIds[it]?.id }
        }.map(::HydrationFastLookupCustomFeeCapability)
    }
}

class HydrationFastLookupCustomFeeCapability(
    private val acceptedCurrencies: Set<ChainAssetId>
) : FastLookupCustomFeeCapability {
    override fun canPayFeeInNonUtilityToken(chainAssetId: ChainAssetId): Boolean {
        return chainAssetId in acceptedCurrencies
    }
}
