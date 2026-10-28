package io.paritytech.polkadotapp.feature_products_impl.domain.derivation

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.RingVrfDerivationProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ringVrfPath
import uniffi.truapi.ProductId

class ReservedRingVrfDerivationProvider(
    private val dotNsTldProvider: DotNsTldProvider,
    private val reservedProductId: (DotNsTld) -> Result<ProductId>,
    private val index: DerivationIndex32,
) : RingVrfDerivationProvider {
    override suspend fun provideDerivationPath(): String {
        val tld = dotNsTldProvider.getTldRetrying()
        return ringVrfPath(reservedProductId(tld).getOrThrow(), index)
    }
}
