package io.paritytech.polkadotapp.tools_backup_api.domain

import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupMetadata
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup

interface BackupService {
    suspend fun saveBackup(backup: Backup, metadata: BackupMetadata): Result<Unit>

    suspend fun getRestorableBackup(): Result<RestorableBackup?>

    suspend fun deleteBackup(): Result<Unit>

    suspend fun resetUserAuthentication()
}
