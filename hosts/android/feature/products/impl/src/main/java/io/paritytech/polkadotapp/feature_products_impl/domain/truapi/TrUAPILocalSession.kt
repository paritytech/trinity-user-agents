package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import javax.inject.Inject

/** Public selection and display metadata for wallet activation. */
data class TrUAPILocalSession(
    val walletId: String,
    val liteUsername: String?,
)

class TrUAPILocalSessionSource @Inject constructor(
    private val accountRepository: AccountRepository,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
) {
    suspend fun resolve(walletId: Long): Result<TrUAPILocalSession> = runCancellableCatching {
        requireSelectedWallet(walletId)
        val username = usernameOfAccountUseCase.getUsername().getOrNull()?.liteUsername?.getDisplayUsername()
        requireSelectedWallet(walletId)
        TrUAPILocalSession(walletId.toString(), username)
    }

    private suspend fun requireSelectedWallet(walletId: Long) {
        val selected = accountRepository.getWalletAccount()
        require(selected.id == walletId && selected.purpose == MetaAccount.Purpose.WALLET) {
            "Wallet selection changed during activation"
        }
    }
}
