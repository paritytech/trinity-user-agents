package io.paritytech.polkadotapp.feature_backup_impl

import io.novasama.substrate_sdk_android.encrypt.mnemonic.MnemonicCreator
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import javax.inject.Inject

interface ManualMnemonicInteractor {
    suspend fun createAccounts(entropy: ByteArray): Result<Unit>

    suspend fun restoreAccountWithEnteredMnemonic(words: String): Result<Unit>
}

class RealManualMnemonicInteractor @Inject constructor(
    private val accountRepository: AccountRepository,
) : ManualMnemonicInteractor {
    override suspend fun createAccounts(entropy: ByteArray): Result<Unit> {
        return runCatching {
            accountRepository.initAccounts(entropy)
        }
    }

    override suspend fun restoreAccountWithEnteredMnemonic(words: String): Result<Unit> {
        return runCatching {
            val mnemonic = MnemonicCreator.fromWords(words)
            accountRepository.initAccounts(mnemonic.entropy)
        }
    }
}
