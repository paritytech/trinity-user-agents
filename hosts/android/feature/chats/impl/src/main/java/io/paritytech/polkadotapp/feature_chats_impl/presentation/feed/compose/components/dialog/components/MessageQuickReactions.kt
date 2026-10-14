package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.common.R as RCommon

private val REACTION_BUTTON_SIZE = 36.dp

@Composable
internal fun QuickReactionsRow(
    state: MessagePopUpUiState.ActionMenu,
    onMessageAction: (MessageAction) -> Unit
) {
    var isExpanded by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxWidth()) {
        PolkadotSurface(
            modifier = Modifier.fillMaxWidth(),
            shape = PolkadotTheme.shapes.large,
            color = PolkadotTheme.colors.bg.surface.container,
            border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.small),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                QUICK_REACTIONS.fastForEach { emoji ->
                    val onClick = remember(emoji) {
                        { onMessageAction(MessageAction.Reaction(state.message, emoji)) }
                    }
                    QuickReactionButton(
                        emoji = emoji,
                        isSelected = emoji in state.userReactedEmojis,
                        onClick = onClick
                    )
                }

                ExpandButton(
                    isExpanded = isExpanded,
                    onClick = { isExpanded = !isExpanded }
                )
            }
        }

        AnimatedVisibility(
            visible = isExpanded,
            enter = expandVertically() + fadeIn(),
            exit = shrinkVertically() + fadeOut()
        ) {
            Column {
                VerticalSpacer { tiny }

                ExpandedEmojiPicker(
                    modifier = Modifier.fillMaxWidth(),
                    userReactedEmojis = state.userReactedEmojis,
                    onEmojiClick = { emoji ->
                        onMessageAction(MessageAction.Reaction(state.message, emoji))
                    }
                )
            }
        }
    }
}

@Composable
private fun QuickReactionButton(
    emoji: String,
    isSelected: Boolean,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = if (isSelected) PolkadotTheme.colors.bg.action.tertiary else PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        Box(
            modifier = Modifier.size(REACTION_BUTTON_SIZE),
            contentAlignment = Alignment.Center
        ) {
            NovaText(
                text = emoji,
                style = PolkadotTheme.typography.headline.small
            )
        }
    }
}

@Composable
private fun ExpandButton(
    isExpanded: Boolean,
    onClick: () -> Unit
) {
    val description = stringResource(RCommon.string.chat_reactions_expand)
    val rotation by animateFloatAsState(
        targetValue = if (isExpanded) 180f else 0f,
        label = "expand_rotation"
    )

    PolkadotSurface(
        modifier = Modifier.semantics { contentDescription = description },
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.action.tertiary,
        onClick = onClick
    ) {
        Box(
            modifier = Modifier.size(REACTION_BUTTON_SIZE),
            contentAlignment = Alignment.Center
        ) {
            NovaIcon(
                modifier = Modifier.rotate(rotation),
                imageVector = NovaIcons.ArrowDropdown,
                tint = PolkadotTheme.colors.fg.primary
            )
        }
    }
}
