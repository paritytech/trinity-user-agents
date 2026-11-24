package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import uniffi.truapi.ProductId
import javax.inject.Inject

/** What the funding screens need from the app beside the core: the balance asset, what it can spend, and provider brands. */
class FundingFlowInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val balance: FundingBalance,
    private val branding: FundingProviderBranding,
) {
    /** The balance asset's decimals. */
    suspend fun cashPrecision(): Int = chainAssetProvider.asset().precision

    suspend fun spendable(): Result<Balance> = balance.spendable()

    suspend fun brand(providerId: ProductId): FundingProviderBrand = branding.brand(providerId)
}
