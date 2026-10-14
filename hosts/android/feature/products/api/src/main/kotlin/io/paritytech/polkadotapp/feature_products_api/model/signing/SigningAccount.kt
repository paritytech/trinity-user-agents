package io.paritytech.polkadotapp.feature_products_api.model.signing

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

sealed interface SigningAccount {
    data class Product(val accountId: ProductAccountId) : SigningAccount

    data object IdentityAccount : SigningAccount

    /**
     * An account the app holds no key for. Only the TrUAPI core produces these,
     * and it signs them itself, so the host can render one but never sign with it.
     */
    data class Legacy(val accountId: AccountId) : SigningAccount
}
