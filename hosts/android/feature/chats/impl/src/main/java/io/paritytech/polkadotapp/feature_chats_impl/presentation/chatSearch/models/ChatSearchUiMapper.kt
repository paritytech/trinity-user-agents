package io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.presentation.search.SearchState
import io.paritytech.polkadotapp.common.presentation.search.map
import io.paritytech.polkadotapp.common.utils.SizedList
import io.paritytech.polkadotapp.common.utils.toSizedList
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.search.ChatListSearchResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.RecentChat
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

internal fun SearchState<SizedList<ChatListSearchResult>>.toSectionsState(
    query: String,
    chatsById: Map<ChatId, Chat>,
): SearchState<SizedList<ChatSearchSectionUiModel>> = map { it.toSearchSections(query, chatsById).toSizedList() }

internal fun List<ChatListSearchResult>.toSearchSections(
    query: String,
    chatsById: Map<ChatId, Chat>,
): ImmutableList<ChatSearchSectionUiModel> {
    val appRows = mutableListOf<ChatSearchRowUiModel>()
    val chatRows = mutableListOf<ChatSearchRowUiModel>()
    val messageRows = mutableListOf<ChatSearchRowUiModel>()

    forEach { result ->
        when (result) {
            is ChatListSearchResult.App -> appRows += result.toRow()
            is ChatListSearchResult.Chat -> chatRows += result.toRow(chatsById)
            is ChatListSearchResult.Message -> messageRows += result.toRow(query, chatsById)
        }
    }

    return buildList {
        addSectionIfNotEmpty("apps", RCommon.string.chat_search_section_apps, appRows)
        addSectionIfNotEmpty("chats", RCommon.string.chat_search_section_chats, chatRows)
        addSectionIfNotEmpty("messages", RCommon.string.chat_search_section_messages, messageRows)
    }.toImmutableList()
}

private fun MutableList<ChatSearchSectionUiModel>.addSectionIfNotEmpty(
    id: String,
    titleRes: Int,
    rows: List<ChatSearchRowUiModel>,
) {
    if (rows.isNotEmpty()) {
        add(ChatSearchSectionUiModel(id, titleRes, rows.toImmutableList()))
    }
}

/**
 * Recents keep only the chat id, so name, avatar and badge always reflect the chat as it is now.
 * A recent whose chat is no longer visible has nothing to render and is dropped.
 */
internal fun RecentChat.toUi(
    isMenuOpen: Boolean,
    chatsById: Map<ChatId, Chat>,
): RecentChatUiModel? = chatsById[chatId]?.toRecentUi(isMenuOpen)

internal fun Chat.toRecentUi(isMenuOpen: Boolean): RecentChatUiModel {
    return RecentChatUiModel(
        chatId = id,
        key = id.uniqueKey(),
        title = display.name,
        avatarModel = avatarModel(),
        status = toRowStatus(),
        isMenuOpen = isMenuOpen,
    )
}

private fun ChatListSearchResult.App.toRow(): ChatSearchRowUiModel {
    return ChatSearchRowUiModel.Person(
        key = "app_$id",
        title = title,
        avatarModel = title.toFallbackAvatarModel(),
        status = NoRowStatus,
        action = ChatSearchRowAction.OpenApp(this),
    )
}

private fun ChatListSearchResult.Chat.toRow(chatsById: Map<ChatId, Chat>): ChatSearchRowUiModel {
    val chat = chatsById[chatId]

    return ChatSearchRowUiModel.Person(
        key = "chat_$id",
        title = title,
        avatarModel = chat?.avatarModel() ?: title.toFallbackAvatarModel(),
        status = chat.toRowStatus(),
        action = ChatSearchRowAction.OpenChat(chatId),
    )
}

private fun ChatListSearchResult.Message.toRow(
    query: String,
    chatsById: Map<ChatId, Chat>,
): ChatSearchRowUiModel {
    val chat = chatsById[chatId]
    val excerpt = buildSearchExcerpt(snippet, query)

    return ChatSearchRowUiModel.Message(
        key = "message_$id",
        title = title,
        avatarModel = chat?.avatarModel() ?: title.toFallbackAvatarModel(),
        status = chat.toRowStatus(),
        action = ChatSearchRowAction.OpenMessage(chatId, messageId),
        timestamp = timestamp,
        snippet = excerpt.text,
        snippetHighlights = excerpt.highlights,
    )
}

private fun Chat.avatarModel(): AvatarUiModel = display.avatar.toUi()

private fun Chat?.toRowStatus(): ChatSearchRowStatus {
    if (this == null) return NoRowStatus

    return ChatSearchRowStatus(
        // TODO: wire mute state from domain Chat once the field exists — mirrors ChatListViewModel.
        isMuted = false,
        hasReaction = hasUnseenReaction,
        badge = unreadBadge.toBadgeUi(),
    )
}

private fun ChatSummaryBadge.toBadgeUi(): ChatListUiState.Badge {
    return when (this) {
        is ChatSummaryBadge.Notification,
        is ChatSummaryBadge.None -> ChatListUiState.Badge.None

        is ChatSummaryBadge.Unread -> ChatListUiState.Badge.Unread(count)
    }
}

private fun String.toFallbackAvatarModel(): AvatarUiModel {
    return AvatarUiModel.Name(
        name = this,
        colorScheme = AvatarColorScheme.from(encodeToByteArray())
    )
}

private fun ChatId.uniqueKey(): String = value.value.toHexString()

internal val NoRowStatus = ChatSearchRowStatus(
    isMuted = false,
    hasReaction = false,
    badge = ChatListUiState.Badge.None,
)
