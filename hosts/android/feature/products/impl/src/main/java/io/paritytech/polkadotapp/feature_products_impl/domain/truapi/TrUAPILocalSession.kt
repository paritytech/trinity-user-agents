package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.WalletSecretProvider
import android.content.Context
import android.app.KeyguardManager
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import javax.inject.Inject

/** Public activation metadata for a protected wallet. */
class TrUAPILocalSession(
    val walletId: String,
    val liteUsername: String?,
)

class TrUAPILocalSessionSource @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    @param:ApplicationContext private val context: Context,
) : WalletSecretProvider {
    suspend fun resolve(): Result<TrUAPILocalSession> = runCatching {
        TrUAPILocalSession(
            walletId = accountRepository.getWalletAccount().id.toString(),
            liteUsername = usernameOfAccountUseCase.getUsername()
                .getOrNull()
                ?.liteUsername
                ?.getDisplayUsername(),
        )
    }

    override suspend fun readWalletRootEntropy(walletId: String): ByteArray {
        check(!context.getSystemService(KeyguardManager::class.java).isDeviceLocked) { "Unlock the device before activating the wallet" }
        val account = accountRepository.getWalletAccount()
        check(account.id.toString() == walletId) { "Selected wallet changed before activation" }
        val entropy = accountSecretsStorage.requireMetaAccountPassphrase(account.id).entropy
        check(accountRepository.getWalletAccount().id == account.id && !context.getSystemService(KeyguardManager::class.java).isDeviceLocked) { "Wallet activation was invalidated" }
        return entropy
    }
}
