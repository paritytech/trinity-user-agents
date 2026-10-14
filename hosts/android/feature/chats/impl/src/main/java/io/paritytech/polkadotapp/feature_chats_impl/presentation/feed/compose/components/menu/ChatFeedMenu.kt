@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu

import androidx.compose.animation.AnimatedContent
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatMenuType

@Composable
fun ChatFeedMenu(
    state: ChatMenuState,
    onDismiss: () -> Unit,
    onCopyUsernameClick: () -> Unit,
    onLeaveChatRequest: () -> Unit,
    onLeaveChatConfirm: () -> Unit,
    onBlockUserRequest: () -> Unit,
    onBlockUserConfirm: () -> Unit
) {
    NovaModalBottomSheet(
        isVisible = state.isVisible,
        onDismissRequest = onDismiss
    ) {
        state.type?.let { type ->
            AnimatedContent(
                targetState = type,
                transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC }
            ) {
                when (it) {
                    is ChatMenuType.MainMenu -> {
                        MainMenuContent(
                            type = it,
                            onDismiss = onDismiss,
                            onCopyUsernameClick = onCopyUsernameClick,
                            onLeaveChatClick = onLeaveChatRequest,
                            onBlockUserClick = onBlockUserRequest
                        )
                    }

                    is ChatMenuType.LeaveConfirmation -> {
                        LeaveConfirmationContent(
                            type = it,
                            onConfirm = onLeaveChatConfirm,
                            onCancel = onDismiss
                        )
                    }

                    is ChatMenuType.BlockConfirmation -> {
                        BlockConfirmationContent(
                            type = it,
                            onConfirm = onBlockUserConfirm,
                            onCancel = onDismiss
                        )
                    }

                    is ChatMenuType.MessageHistory -> {
                        MessageHistoryContent(
                            type = it
                        )
                    }

                    is ChatMenuType.Custom -> {
                        it.renderer.DrawMenu(onDismiss = onDismiss)
                    }
                }
            }
        }
    }
}
