package io.paritytech.polkadotapp.feature_settings_impl.domain.interactors

import io.paritytech.polkadotapp.common.domain.errors.BackupDecryptionKeyNotFoundException
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.AccountSecretsStorage
import io.paritytech.polkadotapp.feature_account_api.data.storage.accountSecrets.requireMetaAccountPassphrase
import io.paritytech.polkadotapp.tools_authentication_api.domain.BiometricsService
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.error.CorruptedBackupException
import io.paritytech.polkadotapp.tools_backup_api.domain.usecase.CreateAndSaveBackupFromMnemonicUseCase
import javax.inject.Inject

interface BackupStatusInteractor {
    suspend fun resolveBackupState(): BackupState

    suspend fun performOneTimeAuthentication(): Result<Unit>

    suspend fun saveBackup(): Result<Unit>
}

class RealBackupStatusInteractor @Inject constructor(
    private val backupService: BackupService,
    private val biometricsService: BiometricsService,
    private val accountRepository: AccountRepository,
    private val accountSecretsStorage: AccountSecretsStorage,
    private val createAndSaveBackupFromMnemonicUseCase: CreateAndSaveBackupFromMnemonicUseCase,
) : BackupStatusInteractor {
    override suspend fun resolveBackupState(): BackupState {
        val restorable = backupService.getRestorableBackup()
            .getOrElse { return BackupState.NoAccess }
            ?: return BackupState.None

        val metaId = accountRepository.getWalletAccount().id
        return restorable.restore().fold(
            onSuccess = { backup ->
                val entropy = accountSecretsStorage.getMetaAccountPassphrase(metaId)?.entropy
                if (entropy != null && !backup.value.contentEquals(entropy)) {
                    BackupState.Conflict(restorable.createdAt)
                } else {
                    BackupState.Available(restorable.createdAt)
                }
            },
            onFailure = { error ->
                when (error) {
                    is CorruptedBackupException -> BackupState.Corrupted
                    is BackupDecryptionKeyNotFoundException -> BackupState.Available(restorable.createdAt)
                    else -> BackupState.NoAccess
                }
            }
        )
    }

    override suspend fun performOneTimeAuthentication(): Result<Unit> =
        biometricsService.performOneTimeAuthentication()

    override suspend fun saveBackup(): Result<Unit> = runCatching {
        val metaAccountId = accountRepository.getWalletAccount().id
        accountSecretsStorage.requireMetaAccountPassphrase(metaAccountId)
    }
        .flatMap {
            createAndSaveBackupFromMnemonicUseCase(it)
        }
        .coerceToUnit()
}
