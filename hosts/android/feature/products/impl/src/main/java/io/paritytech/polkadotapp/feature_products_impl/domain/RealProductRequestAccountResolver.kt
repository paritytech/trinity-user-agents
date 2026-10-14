package io.paritytech.polkadotapp.feature_products_impl.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_products_api.domain.ProductRequestAccountResolver
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
import javax.inject.Inject

class RealProductRequestAccountResolver @Inject constructor(
    private val accountRepository: AccountRepository,
) : ProductRequestAccountResolver {
    override suspend fun resolve(account: AccountId): Result<SigningAccount> = runCatching {
        val identityAccountId = accountRepository.getWalletAccount().defaultAccountId()
        require(account == identityAccountId) { "Unknown legacy account: not the identity account" }

        SigningAccount.IdentityAccount
    }
}
