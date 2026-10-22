package io.paritytech.polkadotapp.feature_settings_impl.domain.settings

import io.paritytech.polkadotapp.feature_chats_api.domain.BlockedContactsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageAccountBackupStatus
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.tools_backup_api.data.BackupExistsStorage
import javax.inject.Inject
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart

class SettingsInteractor @Inject constructor(
    private val backupExistsStorage: BackupExistsStorage,
    private val blockedContactsRepository: BlockedContactsRepository,
    private val recyclingStrategySettings: CoinageRecyclingStrategySettings,
    private val coinageAccountBackupObserver: CoinageAccountBackupObserver,
    private val personChatMode: PersonChatMode,
    private val personChatContacts: PersonChatContacts,
) {
    fun observeBackupExists(): Flow<Boolean> {
        return backupExistsStorage.observeBackupExists()
    }

    // While the chat product runs the chats, its contacts are the ones the user blocks.
    fun subscribeHasBlockedContacts(): Flow<Boolean> = if (personChatMode.isProductChat()) {
        personChatContacts.observeContacts().map { contacts -> contacts.any { it.blocked } }
    } else {
        blockedContactsRepository.subscribeHasBlockedContacts()
    }

    fun observePrivacyMode(): Flow<RecyclingStrategyType> = recyclingStrategySettings.strategyFlow()

    fun observePaymentsBackupStatus(): Flow<CoinageAccountBackupStatus?> = coinageAccountBackupObserver.subscribeStatus().onStart<CoinageAccountBackupStatus?> { emit(null) }

    suspend fun setPrivacyMode(mode: RecyclingStrategyType): Result<Unit> =
        recyclingStrategySettings.setStrategy(mode)
}
