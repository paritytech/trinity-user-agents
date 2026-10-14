package io.paritytech.polkadotapp.tools_backup_api.data

import kotlinx.coroutines.flow.Flow

interface BackupExistsStorage {
    fun observeBackupExists(): Flow<Boolean>

    fun setBackupExists(exists: Boolean)
}
