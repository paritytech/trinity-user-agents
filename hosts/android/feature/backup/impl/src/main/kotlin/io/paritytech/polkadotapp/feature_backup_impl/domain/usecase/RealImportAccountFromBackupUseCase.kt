package io.paritytech.polkadotapp.feature_backup_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_backup_api.domain.error.ImportFromBackupError
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.ImportAccountFromBackupUseCase
import io.paritytech.polkadotapp.feature_backup_impl.domain.toImportFromBackupError
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import javax.inject.Inject

class RealImportAccountFromBackupUseCase @Inject constructor(
    private val backupService: BackupService,
    private val accountRepository: AccountRepository,
    private val backupExistsStorage: BackupExistsStorage,
) : ImportAccountFromBackupUseCase {
    override suspend fun invoke(): Result<Unit> {
        return backupService.getRestorableBackup()
            .flatMap { restorable ->
                restorable?.restore()
                    ?.mapCatching { backup -> accountRepository.initAccounts(backup.value) }
                    ?: Result.failure(ImportFromBackupError.NotFound)
            }
            .mapError { it.toImportFromBackupError() }
            .onSuccess { backupExistsStorage.setBackupExists(true) }
    }
}
