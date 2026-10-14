package io.paritytech.polkadotapp.app.root.domain.debug

import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import javax.inject.Inject

class ClearBackupUseCase @Inject constructor(
    private val backupService: BackupService
) {
    suspend operator fun invoke(): Result<Unit> {
        return backupService.deleteBackup()
    }
}
