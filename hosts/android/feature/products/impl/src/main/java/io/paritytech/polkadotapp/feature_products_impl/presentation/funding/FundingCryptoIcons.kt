package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIconKind
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.ProductId
import uniffi.truapi.fundingNetworkName
import java.util.Locale

data class FundingCryptoIcon(
    val monogram: String,
    val image: String?,
)

fun fundingNetworkIcon(id: String, provider: ProductId?): FundingCryptoIcon =
    FundingCryptoIcon(monogramOf(fundingNetworkName(id)), provider?.let { fundingIconModel(it, FundingIconKind.NETWORK, id) })

fun fundingTokenIcon(symbol: String, provider: ProductId?): FundingCryptoIcon =
    FundingCryptoIcon(monogramOf(symbol), provider?.let { fundingIconModel(it, FundingIconKind.TOKEN, symbol) })

fun FundingFlowState.networkIconProvider(network: String): ProductId? =
    firstProviderRouting { route -> route.offersNetwork(network) }

fun FundingFlowState.tokenIconProvider(symbol: String): ProductId? = firstProviderRouting { route ->
    route.assets.any { it.equals(symbol, ignoreCase = true) } && (network == null || route.offersNetwork(network))
}

private fun FundingFlowState.firstProviderRouting(offers: (FundingRoute) -> Boolean): ProductId? =
    candidates.firstOrNull { candidate -> candidate.routes.any { it.rail == FundingRail.CRYPTO && offers(it) } }?.providerId

private fun FundingRoute.offersNetwork(network: String) = networks.orEmpty().any { it.equals(network, ignoreCase = true) }

private fun monogramOf(name: String) = name.take(1).uppercase(Locale.ROOT)
