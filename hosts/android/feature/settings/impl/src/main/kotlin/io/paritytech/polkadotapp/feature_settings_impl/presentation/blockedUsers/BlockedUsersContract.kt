package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models.BlockedUsersUiState
import kotlinx.coroutines.flow.StateFlow

interface BlockedUsersContract {
    val state: StateFlow<BlockedUsersUiState>

    fun onBackClick()

    fun onUnblockClick(accountId: AccountId)

    fun onChatClick(accountId: AccountId)
}
