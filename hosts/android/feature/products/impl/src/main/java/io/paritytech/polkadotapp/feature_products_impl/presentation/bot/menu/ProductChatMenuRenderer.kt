package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.common.utils.capitalize
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDragHandler
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatPlaceholder
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.icon.vectors.Downloading
import io.paritytech.polkadotapp.design.components.icon.vectors.Leave
import io.paritytech.polkadotapp.design.components.icon.vectors.NotificationsBellOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.SpeakerXMark
import io.paritytech.polkadotapp.design.components.icon.vectors.Upload
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.common.R as RCommon

internal class ProductChatMenuRenderer(
    private val product: Product,
    private val chatId: ChatId,
    /** A host-placed chat offers no removal: it would only hide the row until the next launch. */
    val removable: Boolean,
) : CustomChatMenuRenderer {
    @Composable
    override fun DrawMenu(onDismiss: () -> Unit) {
        val viewModel = hiltViewModel<ProductChatMenuViewModel>()
        val currentPage by viewModel.currentPage.collectAsState()
        val userState by remember(chatId) { viewModel.userState(chatId) }.collectAsState(ChatRoomUserStateModel.NONE)
        val isMuted = userState.isMutedAt(System.currentTimeMillis())

        DisposableEffect(Unit) {
            onDispose { viewModel.onMenuDismissed() }
        }

        AnimatedContent(
            targetState = currentPage,
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC }
        ) { page ->
            when (page) {
                ProductChatMenuPage.MAIN -> MainContent(
                    appName = product.name,
                    isMuted = isMuted,
                    isArchived = userState.archived,
                    onOpenAppClick = {
                        viewModel.onOpenAppClick(product.id)
                        onDismiss()
                    },
                    onMuteClick = {
                        viewModel.onMuteClick(chatId, isMuted)
                        if (isMuted) onDismiss()
                    },
                    onArchiveClick = {
                        viewModel.onArchiveClick(chatId, userState.archived)
                        onDismiss()
                    },
                    removable = removable,
                    onRemoveChatClick = { viewModel.onRemoveChatClick() }
                )

                ProductChatMenuPage.MUTE_DURATION -> MuteDurationContent(
                    onSelect = { duration ->
                        viewModel.onMuteDurationSelected(chatId, duration)
                        onDismiss()
                    },
                )

                ProductChatMenuPage.REMOVE_CONFIRMATION -> RemoveConfirmationContent(
                    onConfirm = { viewModel.onRemoveChatConfirmed(product.id, chatId) },
                    onCancel = { onDismiss() },
                )
            }
        }
    }
}

@Composable
private fun MainContent(
    appName: String,
    isMuted: Boolean,
    isArchived: Boolean,
    onOpenAppClick: () -> Unit,
    onMuteClick: () -> Unit,
    onArchiveClick: () -> Unit,
    removable: Boolean,
    onRemoveChatClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.mediumIncreased
            ),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaBottomSheetDragHandler()

        NovaMenuOption(
            text = stringResource(RCommon.string.product_chat_menu_open_app, appName.capitalize()),
            icon = NovaIcons.ChatPlaceholder,
            color = PolkadotTheme.colors.fg.primary,
            onClick = onOpenAppClick
        )

        // The mute and the archive are the user's: the product hears of them, but cannot set them (D34).
        NovaMenuOption(
            text = stringResource(if (isMuted) RCommon.string.chat_room_unmute else RCommon.string.chat_room_mute),
            icon = if (isMuted) NovaIcons.NotificationsBellOutlined else NovaIcons.SpeakerXMark,
            color = PolkadotTheme.colors.fg.primary,
            onClick = onMuteClick
        )

        NovaMenuOption(
            text = stringResource(if (isArchived) RCommon.string.chat_room_unarchive else RCommon.string.chat_room_archive),
            icon = if (isArchived) NovaIcons.Upload else NovaIcons.Downloading,
            color = PolkadotTheme.colors.fg.primary,
            onClick = onArchiveClick
        )

        if (removable) {
            NovaMenuOption(
                text = stringResource(RCommon.string.product_chat_menu_remove_chat),
                icon = NovaIcons.Leave,
                color = PolkadotTheme.colors.fg.error,
                onClick = onRemoveChatClick
            )
        }
    }
}

/** The durations of a mute (07-LATER §6.2): 1 hour, 8 hours, 1 week, or always. */
@Composable
private fun MuteDurationContent(onSelect: (ChatMuteDuration) -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.mediumIncreased
            ),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaBottomSheetDragHandler()

        NovaText(
            modifier = Modifier.padding(vertical = PolkadotTheme.spacings.medium),
            text = stringResource(RCommon.string.chat_room_mute_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
        )

        ChatMuteDuration.entries.forEach { duration ->
            NovaMenuOption(
                text = stringResource(duration.label()),
                icon = if (duration == ChatMuteDuration.ALWAYS) NovaIcons.SpeakerXMark else NovaIcons.Clock,
                color = PolkadotTheme.colors.fg.primary,
                onClick = { onSelect(duration) }
            )
        }
    }
}

private fun ChatMuteDuration.label(): Int = when (this) {
    ChatMuteDuration.ONE_HOUR -> RCommon.string.chat_room_mute_one_hour
    ChatMuteDuration.EIGHT_HOURS -> RCommon.string.chat_room_mute_eight_hours
    ChatMuteDuration.ONE_WEEK -> RCommon.string.chat_room_mute_one_week
    ChatMuteDuration.ALWAYS -> RCommon.string.chat_room_mute_always
}

@Composable
private fun RemoveConfirmationContent(
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            modifier = Modifier.padding(top = PolkadotTheme.spacings.small),
            text = stringResource(RCommon.string.product_chat_menu_remove_confirmation_text),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { extraLarge }

        Row(
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            val buttonModifier = Modifier.weight(1f)

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.common_cancel),
                style = PolkadotButtonStyle.secondary(),
                onClick = onCancel,
            )

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.product_chat_menu_remove_confirmation_positive),
                style = PolkadotButtonStyle.destructive(),
                onClick = onConfirm,
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MainContentPreview() {
    PolkadotTheme {
        MainContent(
            appName = "Nova Swap",
            isMuted = false,
            isArchived = false,
            onOpenAppClick = {},
            onMuteClick = {},
            onArchiveClick = {},
            removable = true,
            onRemoveChatClick = {}
        )
    }
}
