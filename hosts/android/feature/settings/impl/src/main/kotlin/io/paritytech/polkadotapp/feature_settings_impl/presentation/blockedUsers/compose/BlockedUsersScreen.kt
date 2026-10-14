package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.design.components.avatar.NovaContactItem
import io.paritytech.polkadotapp.design.components.avatar.NovaContactItemType
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.BlockedUsersContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models.BlockedUserUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models.BlockedUsersUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun BlockedUsersScreen(contract: BlockedUsersContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    BlockedUsersScreenInternal(
        state = state,
        onBackClick = contract::onBackClick,
        onUnblockClick = contract::onUnblockClick,
        onChatClick = contract::onChatClick
    )
}

@Composable
private fun BlockedUsersScreenInternal(
    state: BlockedUsersUiState,
    onBackClick: () -> Unit,
    onUnblockClick: (AccountId) -> Unit,
    onChatClick: (AccountId) -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_blocked_contacts),
                navigationAction = rememberTopBarAction(onBackClick),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            LazyColumn {
                items(state.blockedUsers, key = { it.accountId.value.contentToString() }) { user ->
                    BlockedUserItem(
                        user = user,
                        onChatClick = { onChatClick(user.accountId) },
                        onUnblockClick = { onUnblockClick(user.accountId) }
                    )
                }
            }
        }
    }
}

@Composable
private fun BlockedUserItem(
    user: BlockedUserUiModel,
    onChatClick: () -> Unit,
    onUnblockClick: () -> Unit
) {
    NovaContactItem(
        title = user.displayName,
        type = NovaContactItemType.User,
        avatarModel = user.avatarModel,
        onClick = onChatClick,
        endContent = {
            PolkadotTextButton(
                style = PolkadotButtonStyle.ghost(),
                text = stringResource(RCommon.string.blocked_users_unblock),
                onClick = onUnblockClick
            )
        }
    )
}
