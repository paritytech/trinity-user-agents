package io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource

/** How the topped-up funds are obtained when claiming. */
sealed interface TopUpSource {
    /** Onboard (e.g. load a recycler) using a signer key. */
    data class Onboard(val signerSource: TransactionSignerSource.Signed) : TopUpSource

    /** Move already-existing coins into the user's coin set, identified by their secret keys. */
    data class Coins(val coinKeys: List<CoinPrivateKey>) : TopUpSource
}
