package io.paritytech.polkadotapp.tools_backup_impl.data.store.backup

import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupMetadata
import io.paritytech.polkadotapp.tools_backup_impl.data.model.EncryptedBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.RestoredEncryptedBackup

interface EncryptedBackupStore {
    suspend fun isAuthorized(): Boolean

    suspend fun write(encryptedBackup: EncryptedBackup, metadata: BackupMetadata): Result<Unit>

    suspend fun read(): Result<RestoredEncryptedBackup?>

    suspend fun delete(): Result<Unit>

    suspend fun resetUserAuthentication()
}
