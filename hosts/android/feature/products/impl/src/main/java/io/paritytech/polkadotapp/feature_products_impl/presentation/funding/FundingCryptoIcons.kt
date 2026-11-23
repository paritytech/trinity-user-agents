package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIconKind
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.fundingIconModel
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.ProductId
import uniffi.truapi.fundingNetworkName
import java.util.Locale

/**
 * A crypto network's or token's mark: the icon the provider it belongs to ships, or its first letter while that loads
 * or when the provider ships none.
 */
data class FundingCryptoIcon(
    val monogram: String,
    /** The image model of the provider's icon; `null` when no provider offers it or the id names no icon. */
    val image: String?,
)

/** A network's mark, as [provider] ships it. */
fun fundingNetworkIcon(id: String, provider: ProductId?): FundingCryptoIcon =
    FundingCryptoIcon(monogramOf(fundingNetworkName(id)), provider?.let { fundingIconModel(it, FundingIconKind.NETWORK, id) })

/** A token's mark by symbol, as [provider] ships it. */
fun fundingTokenIcon(symbol: String, provider: ProductId?): FundingCryptoIcon =
    FundingCryptoIcon(monogramOf(symbol), provider?.let { fundingIconModel(it, FundingIconKind.TOKEN, symbol) })

/** The provider a network in the picker is drawn by, before one is chosen: the first listed whose routes offer it. */
fun FundingFlowState.networkIconProvider(network: String): ProductId? =
    firstProviderRouting { route -> route.offersNetwork(network) }

/** The provider a token in the picker is drawn by: the first listed whose routes offer it on the network picked. */
fun FundingFlowState.tokenIconProvider(symbol: String): ProductId? = firstProviderRouting { route ->
    route.assets.any { it.equals(symbol, ignoreCase = true) } && (network == null || route.offersNetwork(network))
}

private fun FundingFlowState.firstProviderRouting(offers: (FundingRoute) -> Boolean): ProductId? =
    candidates.firstOrNull { candidate -> candidate.routes.any { it.rail == FundingRail.CRYPTO && offers(it) } }?.providerId

private fun FundingRoute.offersNetwork(network: String) = networks.orEmpty().any { it.equals(network, ignoreCase = true) }

private fun monogramOf(name: String) = name.take(1).uppercase(Locale.ROOT)
