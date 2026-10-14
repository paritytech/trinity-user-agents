package io.paritytech.polkadotapp.feature_backup_impl.recover.domain

import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.ImportAccountFromBackupUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import javax.inject.Inject

interface RecoverOptionsInteractor {
    suspend fun importAccountFromBackup(): Result<Unit>
}

class RealRecoverOptionsInteractor @Inject constructor(
    private val backupService: BackupService,
    private val importAccountFromBackupUseCase: ImportAccountFromBackupUseCase,
) : RecoverOptionsInteractor {
    override suspend fun importAccountFromBackup(): Result<Unit> {
        backupService.resetUserAuthentication()
        return importAccountFromBackupUseCase()
    }
}
