package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.util.isUtilityAsset
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.toOnChainIdOrThrow
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.api.acceptedCurrencies
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.api.multiTransactionPayment
import javax.inject.Inject

internal class HydrationPriceConversionFallback @Inject constructor(
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
) {
    suspend fun convertNativeAmount(amount: Balance, conversionTarget: Chain.Asset): Balance {
        if (conversionTarget.isUtilityAsset) return amount

        val targetOnChainId = hydraDxAssetIdConverter.toOnChainIdOrThrow(conversionTarget)

        val fallbackPrice = remoteStorageSource.query(conversionTarget.chainId) {
            metadata.multiTransactionPayment.acceptedCurrencies.query(targetOnChainId)
        } ?: error("No fallback price found")

        return amount * fallbackPrice.fraction
    }
}
