package io.paritytech.polkadotapp.tools_backup_impl.domain

import io.paritytech.polkadotapp.common.domain.errors.BackupDecryptionKeyNotFoundException
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import io.paritytech.polkadotapp.tools_backup_api.domain.BackupService
import io.paritytech.polkadotapp.tools_backup_api.domain.model.Backup
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupMetadata
import io.paritytech.polkadotapp.tools_backup_api.domain.model.RestorableBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.model.RestoredEncryptedBackup
import io.paritytech.polkadotapp.tools_backup_impl.data.processing.BackupEncryption
import io.paritytech.polkadotapp.tools_backup_impl.data.store.backup.EncryptedBackupStore
import io.paritytech.polkadotapp.tools_backup_impl.data.store.key.BackupEncryptionKeyStore
import kotlinx.coroutines.withContext
import javax.inject.Inject

class RealCloudBackupService @Inject constructor(
    private val encryption: BackupEncryption,
    private val backupExistsStorage: BackupExistsStorage,
    private val backupEncryptionKeyStore: BackupEncryptionKeyStore,
    private val backupStore: EncryptedBackupStore,
    private val coroutineDispatchers: CoroutineDispatchers,
) : BackupService {
    override suspend fun saveBackup(backup: Backup, metadata: BackupMetadata): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            val freshEncryptionKey = encryption.generateNewEncryptionKey()

            backupEncryptionKeyStore.store(freshEncryptionKey)
                .flatMap { encryption.encryptBackup(backup, freshEncryptionKey) }
                .flatMap { backupStore.write(it, metadata) }
                .onSuccess { backupExistsStorage.setBackupExists(true) }
        }
    }

    override suspend fun getRestorableBackup(): Result<RestorableBackup?> {
        return withContext(coroutineDispatchers.io) {
            backupStore.read().mapNotNull(::RealRestorableBackup)
        }
    }

    override suspend fun deleteBackup(): Result<Unit> {
        return withContext(coroutineDispatchers.io) {
            backupStore.delete()
        }
    }

    override suspend fun resetUserAuthentication() {
        backupStore.resetUserAuthentication()
    }

    private inner class RealRestorableBackup(
        private val encryptedBackup: RestoredEncryptedBackup,
    ) : RestorableBackup {
        override val createdAt: Timestamp = encryptedBackup.createdAt

        override suspend fun restore(): Result<Backup> {
            return backupEncryptionKeyStore.read()
                .requireNotNull { BackupDecryptionKeyNotFoundException() }
                .flatMap { encryptionKey ->
                    encryption.decryptBackup(encryptedBackup.backup, encryptionKey)
                }
        }
    }
}
