package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MessageReactions(
    modifier: Modifier = Modifier,
    reactions: ImmutableList<ChatMessageUiModel.Reaction>,
    onReactionClick: (String) -> Unit,
) {
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)
    ) {
        reactions.forEach { reaction ->
            ReactionChip(
                emoji = reaction.emoji,
                count = reaction.count,
                isReactedByTheUser = reaction.reactedByUser,
                onClick = { onReactionClick(reaction.emoji) },
            )
        }
    }
}

@Composable
private fun ReactionChip(
    emoji: String,
    count: Int,
    isReactedByTheUser: Boolean,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = if (isReactedByTheUser) PolkadotTheme.colors.bg.surface.container else PolkadotTheme.colors.bg.action.tertiary,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.cutout),
        onClick = onClick
    ) {
        Row(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.small,
                vertical = PolkadotTheme.spacings.tiny
            ),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
            verticalAlignment = Alignment.CenterVertically
        ) {
            NovaText(
                text = emoji,
                style = PolkadotTheme.typography.body.large
            )

            if (count > 1) {
                NovaText(
                    text = count.toString(),
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.secondary
                )
            }
        }
    }
}

@Preview
@Composable
private fun MessageReactionsPreview() {
    PolkadotTheme {
        MessageReactions(
            reactions = persistentListOf(
                ChatMessageUiModel.Reaction(count = 3, emoji = "👍", reactedByUser = true),
                ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = true),
                ChatMessageUiModel.Reaction(count = 2, emoji = "😂", reactedByUser = true)
            ),
            onReactionClick = {}
        )
    }
}
