package io.paritytech.polkadotapp.feature_products_api.model.signing

import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId

sealed interface SigningAccount {
    data class Product(val accountId: ProductAccountId) : SigningAccount

    data object IdentityAccount : SigningAccount
}
