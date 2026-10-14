package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.collections.immutable.persistentMapOf

private val PreviewIconSize = 16.dp
private const val PREVIEW_ICON_ID = "previewIcon"

@Composable
internal fun ChatItemBody(
    preview: ChatPreviewBody,
    badge: ChatListUiState.Badge,
    hasReaction: Boolean,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.Top,
    ) {
        val previewIcon = preview.icon
        val density = LocalDensity.current

        // Icon is inlined at the text start so wrapped lines flow under it (full width)
        // instead of staying indented past a sibling icon column.
        val text = remember(preview) {
            buildAnnotatedString {
                if (previewIcon != null) {
                    appendInlineContent(PREVIEW_ICON_ID, "icon")
                    append(" ")
                }
                append(preview.text)
            }
        }

        val inlineContent = if (previewIcon != null) {
            persistentMapOf(
                PREVIEW_ICON_ID to InlineTextContent(
                    Placeholder(
                        width = with(density) { PreviewIconSize.toSp() },
                        height = with(density) { PreviewIconSize.toSp() },
                        placeholderVerticalAlign = PlaceholderVerticalAlign.TextCenter,
                    )
                ) {
                    NovaIcon(
                        modifier = Modifier.fillMaxSize(),
                        imageVector = previewIcon,
                        tint = PolkadotTheme.colors.fg.secondary,
                    )
                }
            )
        } else {
            persistentMapOf()
        }

        NovaText(
            modifier = Modifier.weight(1f),
            text = text,
            inlineContent = inlineContent,
            style = PolkadotTheme.typography.paragraph.medium,
            color = PolkadotTheme.colors.fg.secondary,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )

        ChatItemBadges(
            modifier = Modifier.padding(top = PolkadotTheme.spacings.tiny),
            badge = badge,
            hasReaction = hasReaction,
        )
    }
}
