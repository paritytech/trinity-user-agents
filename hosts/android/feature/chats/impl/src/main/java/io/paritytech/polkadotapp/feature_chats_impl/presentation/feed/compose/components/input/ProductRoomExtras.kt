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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.OngoingActivityIndicator
import kotlinx.collections.immutable.ImmutableList

private val CommandCatalogueMaxHeight = 240.dp

/** The activity line of a product room, for example "Bot is typing" (02-DESIGN §6.6). */
@Composable
internal fun ChatActivityLine(
    modifier: Modifier = Modifier,
    text: String,
) {
    Row(
        modifier = modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.tiny),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OngoingActivityIndicator()

        HorizontalSpacer { small }

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
            PolkadotSurface(
                shape = PolkadotTheme.shapes.full,
                color = PolkadotTheme.colors.bg.surface.container,
                border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary),
                onClick = { onSelect(suggestion.suggestionId) },
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
                        .clickable { onSelect(command) }
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
