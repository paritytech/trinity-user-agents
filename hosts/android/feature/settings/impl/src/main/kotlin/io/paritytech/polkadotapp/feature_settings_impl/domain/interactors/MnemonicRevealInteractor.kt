package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import javax.inject.Inject

interface MnemonicRevealInteractor {
    suspend fun getMnemonic(): List<String>
}

class RealMnemonicRevealInteractor @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage
) : MnemonicRevealInteractor {
    override suspend fun getMnemonic(): List<String> {
        val account = accountRepository.getWalletAccount()
        return accountSecretsStorage.requireMetaAccountPassphrase(account.id).wordList
    }
}
