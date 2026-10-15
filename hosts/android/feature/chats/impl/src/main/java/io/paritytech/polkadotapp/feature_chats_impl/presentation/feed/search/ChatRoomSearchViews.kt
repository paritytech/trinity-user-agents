package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.search

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import io.paritytech.polkadotapp.common.utils.rememberCurrentTimeMillisWithDelay
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.Search
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.SearchExcerpt
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlin.time.Duration.Companion.minutes
import io.paritytech.polkadotapp.common.R as RCommon

/** The test tags of the in-room search, for the launcher. */
internal object ChatRoomSearchTags {
    const val ACTION = "chat_room_search_action"
    const val FIELD = "chat_room_search_field"
    const val RESULT = "chat_room_search_result"
    const val NOT_AVAILABLE = "chat_room_search_not_available"
}

/** The search action of the room header. */
@Composable
internal fun ChatRoomSearchButton(onClick: () -> Unit) {
    val label = stringResource(RCommon.string.chat_room_search)
    PolkadotIconButton(
        modifier = Modifier
            .testTag(ChatRoomSearchTags.ACTION)
            .semantics { contentDescription = label },
        icon = NovaIcons.Search,
        onClick = onClick,
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotIconButtonSize.mediumIncreased(),
        shape = PolkadotButtonShape.pill,
    )
}

/** The search field that replaces the room header while the user searches. */
@Composable
internal fun ChatRoomSearchBar(
    query: String,
    onQueryChange: (String) -> Unit,
    onClose: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    val closeLabel = stringResource(RCommon.string.chat_room_search_close)

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PolkadotIconButton(
            modifier = Modifier.semantics { contentDescription = closeLabel },
            icon = NovaIcons.ArrowLeft,
            onClick = onClose,
            style = PolkadotButtonStyle.ghost(),
            size = PolkadotIconButtonSize.mediumIncreased(),
            shape = PolkadotButtonShape.pill,
        )

        Box(
            modifier = Modifier
                .weight(1f)
                .background(PolkadotTheme.colors.bg.surface.container, PolkadotTheme.shapes.full)
                .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.extraMedium),
        ) {
            if (query.isEmpty()) {
                NovaText(
                    text = stringResource(RCommon.string.chat_room_search_placeholder),
                    style = PolkadotTheme.typography.paragraph.medium,
                    color = PolkadotTheme.colors.fg.tertiary,
                )
            }
            BasicTextField(
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag(ChatRoomSearchTags.FIELD)
                    .focusRequester(focusRequester),
                value = query,
                onValueChange = onQueryChange,
                singleLine = true,
                textStyle = PolkadotTheme.typography.paragraph.medium.copy(color = PolkadotTheme.colors.fg.primary),
                cursorBrush = SolidColor(PolkadotTheme.colors.fg.primary),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            )
        }
    }
}

/** The results of the in-room search, over the messages: newest first, each with author, time and the match. */
@Composable
internal fun ChatRoomSearchResults(
    modifier: Modifier = Modifier,
    results: ChatRoomSearchUiState.Results,
    onSelect: (RoomSearchResultUiModel) -> Unit,
    onLoadMore: () -> Unit,
) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(PolkadotTheme.colors.bg.surface.main),
    ) {
        when (results) {
            ChatRoomSearchUiState.Results.Idle -> Unit
            ChatRoomSearchUiState.Results.Loading -> NovaCircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            ChatRoomSearchUiState.Results.NoResults -> SearchNotice(RCommon.string.chat_room_search_no_results, tag = null)
            ChatRoomSearchUiState.Results.NotAvailable ->
                SearchNotice(RCommon.string.chat_room_search_not_available, tag = ChatRoomSearchTags.NOT_AVAILABLE)
            ChatRoomSearchUiState.Results.Failed -> SearchNotice(RCommon.string.chat_search_error_message, tag = null)
            is ChatRoomSearchUiState.Results.Hits -> HitList(results, onSelect, onLoadMore)
        }
    }
}

@Composable
private fun HitList(
    hits: ChatRoomSearchUiState.Results.Hits,
    onSelect: (RoomSearchResultUiModel) -> Unit,
    onLoadMore: () -> Unit,
) {
    val now by rememberCurrentTimeMillisWithDelay(1.minutes)

    LazyColumn(modifier = Modifier.fillMaxSize()) {
        itemsIndexed(hits.items, key = { _, item -> item.messageId }) { index, item ->
            // The next page loads when the user comes near the oldest result.
            if (hits.canLoadMore && index >= hits.items.size - LOAD_MORE_DISTANCE) {
                LaunchedEffect(hits.items.size) { onLoadMore() }
            }
            HitRow(item = item, now = now, onClick = { onSelect(item) })
        }

        if (hits.isLoadingMore) {
            item(key = "loading_more") {
                Box(modifier = Modifier.fillMaxWidth().padding(PolkadotTheme.spacings.medium), contentAlignment = Alignment.Center) {
                    NovaCircularProgressIndicator()
                }
            }
        }
    }
}

@Composable
private fun HitRow(item: RoomSearchResultUiModel, now: Long, onClick: () -> Unit) {
    val author = when (val author = item.author) {
        RoomSearchResultUiModel.Author.You -> stringResource(RCommon.string.chat_search_author_you)
        is RoomSearchResultUiModel.Author.Named -> author.name
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(ChatRoomSearchTags.RESULT)
            .clickable(onClick = onClick)
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased, vertical = PolkadotTheme.spacings.extraMedium),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            NovaText(
                modifier = Modifier.weight(1f),
                text = author,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            NovaText(
                text = LocalChatMessageTimeFormatter.current.formatChatListTime(item.timestamp, now),
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.tertiary,
                maxLines = 1,
            )
        }
        NovaText(
            text = item.excerpt.highlighted(),
            style = PolkadotTheme.typography.paragraph.medium,
            color = PolkadotTheme.colors.fg.secondary,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun SearchExcerpt.highlighted() = buildAnnotatedString {
    append(text)
    val style = SpanStyle(color = PolkadotTheme.colors.fg.primary, fontWeight = FontWeight.SemiBold)
    highlights.forEach { range ->
        if (range.first >= 0 && range.last < text.length) addStyle(style, range.first, range.last + 1)
    }
}

@Composable
private fun SearchNotice(text: Int, tag: String?) {
    Box(modifier = Modifier.fillMaxSize().padding(PolkadotTheme.spacings.mediumIncreased), contentAlignment = Alignment.TopCenter) {
        NovaText(
            modifier = if (tag != null) Modifier.testTag(tag) else Modifier,
            text = stringResource(text),
            style = PolkadotTheme.typography.paragraph.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
    }
}

private const val LOAD_MORE_DISTANCE = 5
