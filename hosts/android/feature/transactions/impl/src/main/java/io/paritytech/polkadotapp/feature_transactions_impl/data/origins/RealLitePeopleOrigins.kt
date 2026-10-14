package io.paritytech.polkadotapp.feature_transactions_impl.data.origins

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.LitePeopleOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.PeopleLiteAuth
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.SetTransactionExtensionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import javax.inject.Inject

class RealLitePeopleOrigins @Inject constructor(
    private val accountRepository: AccountRepository,
) : LitePeopleOrigins {
    override suspend fun asLitePerson(): TransactionOrigin {
        val account = accountRepository.getWalletAccount()
        return SetTransactionExtensionOrigin(
            signerSource = TransactionSignerSource.FromAccount(account),
            transactionExtension = PeopleLiteAuth()
        )
    }
}
