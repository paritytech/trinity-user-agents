package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Downloading
import io.paritytech.polkadotapp.design.components.icon.vectors.NotificationsBellOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.SpeakerXMark
import io.paritytech.polkadotapp.design.components.icon.vectors.Upload
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.coroutines.launch
import io.paritytech.polkadotapp.common.R as RCommon

/** The test tag of a list row that has swipe actions; the launcher finds the row of a room with it. */
internal const val CHAT_LIST_SWIPE_ROW_TAG = "chat_list_swipe_row"

/**
 * A row of the chat list. A product room has two swipe actions (07-LATER §6.2): a swipe to the end mutes or
 * unmutes it, and a swipe to the start archives or unarchives it. The same actions are custom accessibility
 * actions. A person chat has no swipe action (D34).
 */
@Composable
internal fun ChatListRow(
    modifier: Modifier = Modifier,
    chat: ChatListUiState.ChatItem,
    currentTimestamp: Long,
    onClick: () -> Unit,
    onMuteToggle: () -> Unit,
    onArchiveToggle: () -> Unit,
) {
    val userState = chat.userState
    if (userState == null) {
        PolkadotChatListItem(modifier = modifier, chat = chat, onClick = onClick, currentTimestamp = currentTimestamp)
        return
    }

    val muteLabel = stringResource(if (userState.isMuted) RCommon.string.chat_room_unmute else RCommon.string.chat_room_mute)
    val archiveLabel = stringResource(if (userState.isArchived) RCommon.string.chat_room_unarchive else RCommon.string.chat_room_archive)
    val swipeState = rememberSwipeToDismissBoxState()
    val scope = rememberCoroutineScope()

    SwipeToDismissBox(
        modifier = modifier
            .testTag(CHAT_LIST_SWIPE_ROW_TAG)
            .semantics {
                customActions = listOf(
                    CustomAccessibilityAction(muteLabel) { onMuteToggle(); true },
                    CustomAccessibilityAction(archiveLabel) { onArchiveToggle(); true },
                )
            },
        state = swipeState,
        backgroundContent = {
            when (swipeState.dismissDirection) {
                SwipeToDismissBoxValue.StartToEnd -> SwipeActionBackground(
                    label = muteLabel,
                    icon = if (userState.isMuted) NovaIcons.NotificationsBellOutlined else NovaIcons.SpeakerXMark,
                    alignment = Alignment.Start,
                )

                SwipeToDismissBoxValue.EndToStart -> SwipeActionBackground(
                    label = archiveLabel,
                    icon = if (userState.isArchived) NovaIcons.Upload else NovaIcons.Downloading,
                    alignment = Alignment.End,
                )

                SwipeToDismissBoxValue.Settled -> Unit
            }
        },
        onDismiss = { direction ->
            when (direction) {
                SwipeToDismissBoxValue.StartToEnd -> onMuteToggle()
                SwipeToDismissBoxValue.EndToStart -> onArchiveToggle()
                SwipeToDismissBoxValue.Settled -> Unit
            }
            // The row stays in place: the action changes the state of the room, not the list of rows.
            scope.launch { swipeState.reset() }
        },
    ) {
        PolkadotChatListItem(
            modifier = Modifier.background(PolkadotTheme.colors.bg.surface.main),
            chat = chat,
            onClick = onClick,
            currentTimestamp = currentTimestamp,
        )
    }
}

@Composable
private fun SwipeActionBackground(label: String, icon: ImageVector, alignment: Alignment.Horizontal) {
    Row(
        modifier = Modifier
            .fillMaxSize()
            .background(PolkadotTheme.colors.bg.surface.container)
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        horizontalArrangement = if (alignment == Alignment.Start) Arrangement.Start else Arrangement.End,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            NovaIcon(imageVector = icon, tint = PolkadotTheme.colors.fg.primary)
            NovaText(
                modifier = Modifier.padding(top = PolkadotTheme.spacings.tiny),
                text = label,
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}
