package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.common.utils.rememberCurrentTimeMillisWithDelay
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchRowAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchRowUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.ChatSearchSectionUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatItemBadges
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatItemHeader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.ChatListAvatarSize
import kotlinx.collections.immutable.ImmutableList
import kotlin.time.Duration.Companion.minutes

@Composable
internal fun ChatSearchResultsSections(
    sections: ImmutableList<ChatSearchSectionUiModel>,
    onRowClick: (ChatSearchRowAction) -> Unit,
) {
    val currentTimestamp by rememberCurrentTimeMillisWithDelay(1.minutes)

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = LocalAppNavigationBarInsets.current.asPaddingValues()
    ) {
        sections.fastForEach { section ->
            item(key = "header_${section.key}") {
                SearchSectionHeader(stringResource(section.titleRes))
            }

            itemsIndexed(
                items = section.rows,
                key = { _, row -> "${section.key}_${row.key}" }
            ) { index, row ->
                Column {
                    SearchResultRow(
                        row = row,
                        currentTimestamp = currentTimestamp,
                        onClick = { onRowClick(row.action) },
                    )

                    if (index < section.rows.lastIndex) {
                        SearchRowDivider(startInset = row.dividerStartInset())
                    }
                }
            }
        }
    }
}

@Composable
private fun SearchResultRow(
    row: ChatSearchRowUiModel,
    currentTimestamp: Long,
    onClick: () -> Unit,
) {
    when (row) {
        is ChatSearchRowUiModel.Person -> ChatSearchPersonRow(
            title = row.title,
            avatarModel = row.avatarModel,
            status = row.status,
            onClick = onClick,
        )

        is ChatSearchRowUiModel.Message -> MessageResultRow(
            row = row,
            currentTimestamp = currentTimestamp,
            onClick = onClick,
        )
    }
}

@Composable
private fun SearchSectionHeader(title: String) {
    NovaText(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.extraMedium,
                vertical = PolkadotTheme.spacings.small
            ),
        text = title,
        style = PolkadotTheme.typography.caption.medium,
        color = PolkadotTheme.colors.fg.secondary,
    )
}

@Composable
private fun SearchRowDivider(startInset: Dp) {
    HorizontalDivider(
        modifier = Modifier.padding(
            start = startInset,
            end = PolkadotTheme.spacings.extraMedium
        )
    )
}

@Composable
private fun MessageResultRow(
    row: ChatSearchRowUiModel.Message,
    currentTimestamp: Long,
    onClick: () -> Unit,
) {
    val timestampText = LocalChatMessageTimeFormatter.current.formatChatListTime(row.timestamp, currentTimestamp)

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(PolkadotTheme.spacings.extraMedium),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.Top,
    ) {
        PolkadotAvatar(
            modifier = Modifier.size(ChatListAvatarSize),
            model = row.avatarModel,
        )

        Column(modifier = Modifier.weight(1f)) {
            ChatItemHeader(
                title = row.title,
                timestamp = timestampText,
                isMuted = row.status.isMuted,
            )

            Row(
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                verticalAlignment = Alignment.Top,
            ) {
                NovaText(
                    modifier = Modifier.weight(1f),
                    text = row.snippet.withHighlights(row.snippetHighlights),
                    style = PolkadotTheme.typography.paragraph.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )

                ChatItemBadges(
                    modifier = Modifier.padding(top = PolkadotTheme.spacings.tiny),
                    badge = row.status.badge,
                    hasReaction = row.status.hasReaction,
                )
            }
        }
    }
}

@Composable
private fun String.withHighlights(highlights: ImmutableList<IntRange>): AnnotatedString {
    val highlightColor = PolkadotTheme.colors.fg.primary
    return remember(this, highlights, highlightColor) {
        buildAnnotatedString {
            append(this@withHighlights)

            highlights
                .filter { it.last < this@withHighlights.length }
                .forEach { addStyle(SpanStyle(color = highlightColor), it.first, it.last + 1) }
        }
    }
}

@Composable
private fun ChatSearchRowUiModel.dividerStartInset(): Dp {
    val rowPadding = PolkadotTheme.spacings.extraMedium

    return when (this) {
        is ChatSearchRowUiModel.Person -> ChatSearchPersonAvatarSize + rowPadding * 2
        is ChatSearchRowUiModel.Message -> ChatListAvatarSize + rowPadding * 2
    }
}
