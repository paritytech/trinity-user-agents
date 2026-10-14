package io.paritytech.polkadotapp.feature_backup_impl.domain

import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.common.presentation.tabs.TabWarningProvider
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class BackupTabWarningProvider @Inject constructor(
    private val backupExistsStorage: BackupExistsStorage
) : TabWarningProvider {
    override val tab: BottomTab = BottomTab.SETTINGS

    override fun observeWarning(): Flow<Boolean> =
        backupExistsStorage.observeBackupExists().map { backupExists -> !backupExists }
}
