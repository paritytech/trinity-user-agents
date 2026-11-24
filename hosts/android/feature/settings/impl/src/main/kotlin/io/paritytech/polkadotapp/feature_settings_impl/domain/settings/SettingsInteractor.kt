package io.paritytech.polkadotapp.feature_settings_impl.domain.settings

import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_settings_api.data.storage.DockletAutoHideStorage
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.onStart
import javax.inject.Inject

class SettingsInteractor @Inject constructor(
    private val backupExistsStorage: BackupExistsStorage,
    private val blockedContactsRepository: BlockedContactsRepository,
    private val recyclingStrategySettings: CoinageRecyclingStrategySettings,
    private val coinageAccountBackupObserver: CoinageAccountBackupObserver,
    private val dockletAutoHideStorage: DockletAutoHideStorage,
) {
    fun observeBackupExists(): Flow<Boolean> {
        return backupExistsStorage.observeBackupExists()
    }

    fun subscribeHasBlockedContacts(): Flow<Boolean> = blockedContactsRepository.subscribeHasBlockedContacts()

    fun observePrivacyMode(): Flow<RecyclingStrategyType> = recyclingStrategySettings.strategyFlow()

    fun observePaymentsBackupStatus(): Flow<CoinageAccountBackupStatus?> = coinageAccountBackupObserver.subscribeStatus().onStart<CoinageAccountBackupStatus?> { emit(null) }

    fun observeDockletAutoHide(): StateFlow<Boolean> = dockletAutoHideStorage.enabled

    fun setDockletAutoHide(enabled: Boolean) {
        dockletAutoHideStorage.setEnabled(enabled)
    }

    suspend fun setPrivacyMode(mode: RecyclingStrategyType): Result<Unit> =
        recyclingStrategySettings.setStrategy(mode)
}
