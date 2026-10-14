package io.paritytech.polkadotapp.feature_products_impl.domain.derivation

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.AccountDerivationProvider
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.productAccountPath

/**
 * RFC-0022: a built-in account is the first account of the product that owns the feature.
 * Reserved ids live in the network's namespace, so the path awaits the settled TLD —
 * a path derived from a guessed TLD would mint keys that belong to no network.
 */
class ReservedProductAccountDerivationProvider(
    private val dotNsTldProvider: DotNsTldProvider,
    private val reservedProductId: (DotNsTld) -> ProductId,
) : AccountDerivationProvider {
    override suspend fun provideDerivationPath(): String {
        val tld = dotNsTldProvider.getTldRetrying()
        return productAccountPath(reservedProductId(tld), DerivationIndex32.default())
    }
}
