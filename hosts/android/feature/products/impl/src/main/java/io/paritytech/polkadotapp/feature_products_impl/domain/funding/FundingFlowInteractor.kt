package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import uniffi.truapi.ProductId
import java.math.BigDecimal
import javax.inject.Inject

/** What the funding screens need from the app beside the core: the balance asset, what it can spend, and provider brands. */
class FundingFlowInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val balance: FundingBalance,
    private val branding: FundingProviderBranding,
) {
    suspend fun cash(): FundingCash {
        val asset = chainAssetProvider.asset()
        return FundingCash(precision = asset.precision)
    }

    suspend fun spendable(cash: FundingCash): BigDecimal? =
        balance.spendable()?.let { cash.decimal(it.value.toString()) }

    suspend fun brand(providerId: ProductId): FundingProviderBrand = branding.brand(providerId)
}
