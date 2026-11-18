package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import uniffi.truapi.FundingRail
import uniffi.truapi.ProductId

/** What the user picked on the funding sheet; `null` where they have not, so the core's facts stand. */
data class FundingPicks(
    val rail: FundingRail?,
    val amountText: String?,
    val country: FundingCountry?,
    val network: String?,
    val asset: String?,
    val providerId: ProductId?,
) {
    fun withRail(newRail: FundingRail): FundingPicks =
        if (newRail == rail) this else copy(rail = newRail, network = null, asset = null)

    fun withAmountText(text: String): FundingPicks = copy(amountText = text)

    fun withCountry(newCountry: FundingCountry): FundingPicks = copy(country = newCountry)

    fun withNetwork(id: String): FundingPicks = copy(network = id, asset = null)

    fun withAsset(symbol: String): FundingPicks = copy(asset = symbol)

    fun withChosenProvider(id: ProductId): FundingPicks = copy(providerId = id)

    /** [base], the session as the core holds it, with these picks over it. */
    fun applyTo(base: FundingFlowState): FundingFlowState {
        val withRail = rail?.let(base::withRail) ?: base
        val withAmount = amountText?.let(withRail::withAmountText) ?: withRail
        val withCountry = country?.let(withAmount::withCountry) ?: withAmount
        val withNetwork = network?.let(withCountry::withNetwork) ?: withCountry
        val withAsset = asset?.let(withNetwork::withAsset) ?: withNetwork
        return providerId?.let(withAsset::withChosenProvider) ?: withAsset
    }

    companion object {
        val NONE = FundingPicks(rail = null, amountText = null, country = null, network = null, asset = null, providerId = null)
    }
}
