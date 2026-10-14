package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import javax.inject.Inject

interface BackupConflictInteractor {
    suspend fun overrideBackup(): Result<Unit>
}

class RealBackupConflictInteractor @Inject constructor(
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val createAndSaveBackupFromMnemonicUseCase: CreateAndSaveBackupFromMnemonicUseCase
) : BackupConflictInteractor {
    override suspend fun overrideBackup(): Result<Unit> = runCatching {
        val candidateAccount = accountRepository.getWalletAccount()
        accountSecretsStorage.requireMetaAccountPassphrase(candidateAccount.id)
    }
        .flatMap { createAndSaveBackupFromMnemonicUseCase(it) }
        .coerceToUnit()
}
