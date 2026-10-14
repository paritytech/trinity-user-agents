package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.randomBytes
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.MessageUnreadFilled
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.ChatTestTags
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatRequestsList.models.ChatRequestsListUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlin.random.Random
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ChatRequestListItem(
    modifier: Modifier = Modifier,
    request: ChatRequestsListUiState.ChatRequestItem,
    onClick: () -> Unit,
    onDeclineClick: () -> Unit
) {
    val timeFormatter = LocalChatMessageTimeFormatter.current

    PolkadotSurface(
        modifier = modifier.testTag(ChatTestTags.CHAT_REQUEST_LIST_ITEM),
        color = PolkadotTheme.colors.bg.surface.main,
        onClick = onClick
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(
                    vertical = PolkadotTheme.spacings.extraMedium,
                    horizontal = PolkadotTheme.spacings.mediumIncreased
                ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            PolkadotAvatar(
                modifier = Modifier.size(56.dp),
                model = request.display.avatarModel
            )

            HorizontalSpacer { mediumIncreased }

            Column(
                modifier = Modifier.weight(1f)
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    NovaText(
                        modifier = Modifier.weight(1f),
                        text = request.display.username,
                        style = PolkadotTheme.typography.title.medium,
                        color = PolkadotTheme.colors.fg.primary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )

                    NovaText(
                        text = timeFormatter.formatChatListTime(request.timestamp),
                        style = PolkadotTheme.typography.body.medium,
                        color = PolkadotTheme.colors.fg.secondary,
                        maxLines = 1,
                    )
                }

                VerticalSpacer { extraTiny }

                Row(
                    horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    NovaText(
                        modifier = Modifier.weight(1f),
                        text = stringResource(RCommon.string.chat_request_label),
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.primary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )

                    ChatIcon()

                    DeclineButton(onClick = onDeclineClick)
                }
            }
        }
    }
}

@Composable
private fun ChatIcon() {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.fg.primary,
    ) {
        NovaIcon(
            modifier = Modifier
                .padding(6.dp)
                .size(12.dp),
            imageVector = NovaIcons.MessageUnreadFilled,
            tint = PolkadotTheme.colors.bg.surface.main
        )
    }
}

@Composable
private fun DeclineButton(
    onClick: () -> Unit
) {
    PolkadotTextButton(
        text = stringResource(RCommon.string.chat_request_decline),
        onClick = onClick,
        style = PolkadotButtonStyle.secondary(),
        size = PolkadotButtonSize.custom(
            padding = PaddingValues(
                horizontal = PolkadotTheme.spacings.extraMedium,
                vertical = PolkadotTheme.spacings.small
            ),
            textStyle = PolkadotTheme.typography.body.smallEmphasized
        )
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatRequestListItemPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            ChatRequestListItem(
                request = ChatRequestsListUiState.ChatRequestItem(
                    accountId = Random.randomBytes(32).intoAccountId(),
                    display = ChatDisplayUiModel(
                        username = "alice.polkadot",
                        avatarModel = AvatarUiModel.Mock.fromName("alice.polkadot")
                    ),
                    timestamp = System.currentTimeMillis()
                ),
                onClick = {},
                onDeclineClick = {}
            )
        }
    }
}
