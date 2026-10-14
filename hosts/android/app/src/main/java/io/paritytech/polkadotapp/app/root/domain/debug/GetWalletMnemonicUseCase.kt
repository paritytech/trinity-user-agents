package io.paritytech.polkadotapp.app.root.domain.debug

import io.novasama.substrate_sdk_android.encrypt.mnemonic.Mnemonic
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import javax.inject.Inject

class GetWalletMnemonicUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
) {
    suspend operator fun invoke(): Mnemonic {
        val accountId = accountRepository.getWalletAccount().id
        return accountSecretsStorage.requireMetaAccountPassphrase(accountId)
    }
}
