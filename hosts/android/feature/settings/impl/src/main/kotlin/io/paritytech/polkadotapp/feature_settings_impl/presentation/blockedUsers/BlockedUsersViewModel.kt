package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.BlockedUsersInteractor
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models.BlockedUserUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models.BlockedUsersUiState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toPersistentList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class BlockedUsersViewModel @Inject constructor(
    private val interactor: BlockedUsersInteractor,
    private val router: SettingsRouter
) : BaseViewModel(), BlockedUsersContract {
    override val state: StateFlow<BlockedUsersUiState> = interactor.subscribeBlockedContacts()
        .map { contacts ->
            BlockedUsersUiState(blockedUsers = contacts.map { it.toUiModel() }.toPersistentList())
        }
        .stateIn(
            scope = this,
            started = SharingStarted.Eagerly,
            initialValue = BlockedUsersUiState(blockedUsers = persistentListOf())
        )

    override fun onBackClick() {
        router.back()
    }

    override fun onUnblockClick(accountId: AccountId) = launchUnit {
        interactor.unblockContact(accountId)
    }

    override fun onChatClick(accountId: AccountId) {
        router.openContactChat(accountId)
    }

    private fun Contact.toUiModel(): BlockedUserUiModel {
        val displayName = username ?: accountId.value.toHexShort()
        val avatarUrl = avatarUrl
        val avatarModel = if (avatarUrl != null) {
            AvatarUiModel.Image(avatarUrl)
        } else {
            AvatarUiModel.Name(displayName, AvatarColorScheme.from(accountId.value))
        }
        return BlockedUserUiModel(
            accountId = accountId,
            displayName = displayName,
            avatarModel = avatarModel
        )
    }

    private fun ByteArray.toHexShort(): String {
        return take(4).joinToString("") { "%02x".format(it) }
    }
}
