package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.EmojiReactionGroup
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessagePopUpUiState
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReactionDetail
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ReactionDetailsDialog(
    reactionDetails: MessagePopUpUiState.ReactionsDetails,
    onDismiss: () -> Unit,
    onBack: () -> Unit,
) {
    Dialog(onDismissRequest = onDismiss) {
        ReactionDetailsContent(
            reactionDetails = reactionDetails,
            onBack = onBack,
        )
    }
}

@Composable
private fun ReactionDetailsContent(
    reactionDetails: MessagePopUpUiState.ReactionsDetails,
    onBack: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.mediumIncreased,
        color = PolkadotTheme.colors.bg.surface.main,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary)
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(
                        top = PolkadotTheme.spacings.mediumIncreased,
                        start = PolkadotTheme.spacings.mediumIncreased,
                        end = PolkadotTheme.spacings.mediumIncreased
                    ),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(onClick = onBack) {
                    NovaIcon(
                        imageVector = NovaIcons.ArrowLeft,
                        tint = PolkadotTheme.colors.fg.primary
                    )
                }

                Column(
                    modifier = Modifier.weight(1f),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    NovaText(
                        text = stringResource(RCommon.string.chat_reactions_details_title),
                        style = PolkadotTheme.typography.title.large,
                        color = PolkadotTheme.colors.fg.primary
                    )

                    NovaText(
                        text = pluralStringResource(
                            RCommon.plurals.chat_reactions_details_total,
                            reactionDetails.totalReactionsCount,
                            reactionDetails.totalReactionsCount
                        ),
                        style = PolkadotTheme.typography.body.medium,
                        color = PolkadotTheme.colors.fg.secondary
                    )
                }

                HorizontalSpacer { LocalMinimumInteractiveComponentSize.current }
            }

            LazyColumn(
                contentPadding = PaddingValues(
                    start = PolkadotTheme.spacings.mediumIncreased,
                    end = PolkadotTheme.spacings.mediumIncreased,
                    bottom = PolkadotTheme.spacings.mediumIncreased
                ),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
            ) {
                items(reactionDetails.reactionsByEmoji) { group ->
                    EmojiGroupSection(group = group)
                }
            }
        }
    }
}

@Composable
private fun EmojiGroupSection(
    group: EmojiReactionGroup
) {
    Column(
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically
        ) {
            NovaText(
                text = group.emoji,
                style = PolkadotTheme.typography.title.large
            )

            NovaText(
                text = group.reactions.size.toString(),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        Column(
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)
        ) {
            group.reactions.forEach { reaction ->
                ReactionItem(reaction = reaction)
            }
        }
    }
}

@Composable
private fun ReactionItem(
    reaction: ReactionDetail,
    modifier: Modifier = Modifier
) {
    val timeFormatter = LocalTimeFormatter.current

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.tiny),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
    ) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically
        ) {
            PolkadotAvatar(
                modifier = Modifier.size(24.dp),
                model = reaction.avatarModel,
            )

            NovaText(
                text = reaction.authorName,
                style = if (reaction.isUser) {
                    PolkadotTheme.typography.body.mediumEmphasized
                } else {
                    PolkadotTheme.typography.body.large
                },
                color = PolkadotTheme.colors.fg.primary
            )
        }

        NovaText(
            text = timeFormatter.formatTime(reaction.timestamp),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary
        )
    }
}

@Preview
@Composable
private fun ReactionDetailsPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current),
        ) {
            ReactionDetailsContent(
                onBack = {},
                reactionDetails = MessagePopUpUiState.ReactionsDetails(
                    messageId = "1",
                    totalReactionsCount = 5,
                    reactionsByEmoji = persistentListOf(
                        EmojiReactionGroup(
                            emoji = "👍",
                            reactions = listOf(
                                ReactionDetail(
                                    emoji = "👍",
                                    authorName = "You",
                                    timestamp = System.currentTimeMillis(),
                                    isUser = true,
                                    avatarModel = AvatarUiModel.Mock.fromName("You"),
                                ),
                                ReactionDetail(
                                    emoji = "👍",
                                    authorName = "Alice",
                                    timestamp = System.currentTimeMillis() - 60000,
                                    isUser = false,
                                    avatarModel = AvatarUiModel.Mock.fromName("Alice"),
                                ),
                                ReactionDetail(
                                    emoji = "👍",
                                    authorName = "Bob",
                                    timestamp = System.currentTimeMillis() - 120000,
                                    isUser = false,
                                    avatarModel = AvatarUiModel.Mock.fromName("Bob"),
                                )
                            ).toImmutableList()
                        ),
                        EmojiReactionGroup(
                            emoji = "❤️",
                            reactions = listOf(
                                ReactionDetail(
                                    emoji = "❤️",
                                    authorName = "Charlie",
                                    timestamp = System.currentTimeMillis() - 180000,
                                    isUser = false,
                                    avatarModel = AvatarUiModel.Mock.fromName("Charlie"),
                                ),
                                ReactionDetail(
                                    emoji = "❤️",
                                    authorName = "Diana",
                                    timestamp = System.currentTimeMillis() - 240000,
                                    isUser = false,
                                    avatarModel = AvatarUiModel.Mock.fromName("Diana"),
                                )
                            ).toImmutableList()
                        )
                    )
                )
            )
        }
    }
}
