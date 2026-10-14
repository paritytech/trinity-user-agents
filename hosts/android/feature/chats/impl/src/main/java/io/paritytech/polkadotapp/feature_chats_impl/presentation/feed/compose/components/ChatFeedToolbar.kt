package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.More
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarAction
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatToolbarAction
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toPersistentList

@Composable
internal fun ChatFeedToolbar(
    displayState: LoadingState<ChatDisplayUiModel>,
    toolbarActions: ImmutableList<ChatToolbarAction>,
    showAvatar: Boolean,
    onBack: () -> Unit,
    onStartCallClick: (withVideo: Boolean) -> Unit,
    onMenuClick: () -> Unit
) {
    val isLoaded = displayState is LoadingState.Loaded
    val displayData = displayState.dataOrNull
    val avatarModel = displayData?.avatarModel?.takeIf { showAvatar }

    val leadingAvatar: (@Composable () -> Unit)? = if (avatarModel != null) {
        { PolkadotAvatar(model = avatarModel, modifier = Modifier.fillMaxSize()) }
    } else {
        null
    }

    val resolvedActions: ImmutableList<TopBarAction> = if (!isLoaded) {
        persistentListOf()
    } else {
        toolbarActions.map { action ->
            key(action) {
                when (action) {
                    ChatToolbarAction.AUDIO_CALL -> rememberTopBarAction(
                        action = { onStartCallClick(false) },
                        icon = NovaIcons.CallFilled
                    )
                    ChatToolbarAction.VIDEO_CALL -> rememberTopBarAction(
                        action = { onStartCallClick(true) },
                        icon = NovaIcons.VideocamFilled
                    )
                    ChatToolbarAction.MENU -> rememberTopBarAction(
                        action = onMenuClick,
                        icon = NovaIcons.More
                    )
                }
            }
        }.toPersistentList()
    }

    PolkadotTopBar(
        navigationAction = rememberTopBarAction(action = onBack, icon = NovaIcons.ArrowLeft),
        title = displayData?.username,
        subtitle = null,
        actions = resolvedActions,
        leadingContent = leadingAvatar,
    )
}
