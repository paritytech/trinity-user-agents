package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.input

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.OngoingActivityIndicator
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

private val CommandCatalogueMaxHeight = 240.dp

private val ActivityAvatarSize = 16.dp

/**
 * The activity line of a product room, for example "Bot is typing" (02-DESIGN §6.6). A line about a persona shows the
 * icon of the persona (07-LATER §5.2).
 */
@Composable
internal fun ChatActivityLine(
    modifier: Modifier = Modifier,
    text: String,
    avatar: AvatarUiModel? = null,
) {
    Row(
        modifier = modifier
            // TalkBack tells the user when the activity changes, for example when the product starts to type.
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.tiny),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OngoingActivityIndicator()

        HorizontalSpacer { small }

        if (avatar != null) {
            PolkadotAvatar(model = avatar, modifier = Modifier.size(ActivityAvatarSize))
            HorizontalSpacer { tiny }
        }

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The quick replies of a product room, as chips in a strip above the input (02-DESIGN §6.6). */
@Composable
internal fun ChatSuggestionsStrip(
    modifier: Modifier = Modifier,
    suggestions: ImmutableList<ChatSuggestionModel>,
    onSelect: (suggestionId: String) -> Unit,
) {
    Row(
        modifier = modifier
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.tiny),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        suggestions.forEach { suggestion ->
            val shape = PolkadotTheme.shapes.full
            // A quick reply that sends its text says so, so the user knows that a tap sends a message.
            val actionLabel = stringResource(if (suggestion.sendsText) RCommon.string.common_send else RCommon.string.chat_suggestion_select)
            PolkadotSurface(
                modifier = Modifier
                    .clip(shape)
                    .clickable(role = Role.Button, onClickLabel = actionLabel) { onSelect(suggestion.suggestionId) },
                shape = shape,
                color = PolkadotTheme.colors.bg.surface.container,
                border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
            ) {
                NovaText(
                    modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
                    text = suggestion.text,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1,
                )
            }
        }
    }
}

/** The slash commands that match what the user typed after `/`, with their argument hint (02-DESIGN §6.6). */
@Composable
internal fun ChatCommandCatalogue(
    modifier: Modifier = Modifier,
    commands: ImmutableList<ChatCommandModel>,
    onSelect: (ChatCommandModel) -> Unit,
) {
    PolkadotSurface(
        modifier = modifier.padding(horizontal = PolkadotTheme.spacings.small),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
    ) {
        LazyColumn(modifier = Modifier.heightIn(max = CommandCatalogueMaxHeight)) {
            items(items = commands, key = { it.name }) { command ->
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(role = Role.Button, onClickLabel = stringResource(RCommon.string.chat_command_insert)) { onSelect(command) }
                        .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.small),
                ) {
                    val hint = command.argumentHint?.let { " $it" }.orEmpty()
                    NovaText(
                        text = "/${command.name}$hint",
                        style = PolkadotTheme.typography.body.mediumEmphasized,
                        color = PolkadotTheme.colors.fg.primary,
                    )
                    NovaText(
                        text = command.description,
                        style = PolkadotTheme.typography.body.small,
                        color = PolkadotTheme.colors.fg.tertiary,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}
