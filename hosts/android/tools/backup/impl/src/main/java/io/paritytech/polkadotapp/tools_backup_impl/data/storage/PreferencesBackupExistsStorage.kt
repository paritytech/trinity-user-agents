package io.paritytech.polkadotapp.tools_backup_impl.data.storage

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class PreferencesBackupExistsStorage @Inject constructor(
    private val preferences: Preferences,
) : BackupExistsStorage {
    override fun observeBackupExists(): Flow<Boolean> = preferences.booleanFlow(BACKUP_EXISTS_KEY, false)

    override fun setBackupExists(exists: Boolean) {
        preferences.putBoolean(BACKUP_EXISTS_KEY, exists)
    }

    companion object {
        private const val BACKUP_EXISTS_KEY = "backup_exists"
    }
}
