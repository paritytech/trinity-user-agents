package io.paritytech.polkadotapp.feature_products_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount

/**
 * Reverse-resolves a raw [AccountId] supplied by a product (legacy signing) to a [SigningAccount]
 * the wallet controls. Resolution happens before any signing UI is shown; a failure means the
 * account is not one we can sign with and the request must be rejected.
 */
interface ProductRequestAccountResolver {
    suspend fun resolve(account: AccountId): Result<SigningAccount>
}
