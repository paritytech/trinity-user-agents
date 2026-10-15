package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models

import androidx.compose.runtime.Immutable
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.ChatDisplayUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ResolvedChatImage
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class ChatListUiState(
    /** The rows of the section that shows: the rooms that are not archived, or the archived rooms. */
    val chats: ImmutableList<ChatItem>,
    val pendingRequestsCount: Int,
    /** The number of archived rooms. The main section shows an "Archived" row when it is not zero (07-LATER §6.2). */
    val archivedCount: Int = 0,
    /** True when the list shows the archived rooms. */
    val showsArchived: Boolean = false,
) {
    val hasNewRequests: Boolean = !showsArchived && pendingRequestsCount > 0

    val hasArchivedRow: Boolean = !showsArchived && archivedCount > 0

    data class ChatItem(
        val chatId: ChatId,
        val display: ChatDisplayUiModel,
        val badge: Badge,
        val preview: ChatPreviewUiModel?,
        val isMuted: Boolean,
        val hasReaction: Boolean,
        /** The preview is an activity that continues, for example "Bot is typing". */
        val isOngoing: Boolean,
        /** The mute and the archive that the user can change. Null for a person chat, which has neither (D34). */
        val userState: UserState? = null,
    ) {
        val uniqueKey: String = chatId.value.value.toHexString()
    }

    /** What the user set for a product room, at the time of the list. */
    data class UserState(val isMuted: Boolean, val isArchived: Boolean)

    sealed interface Badge {
        data class Unread(
            val count: Int
        ) : Badge

        data class Image(val image: ResolvedChatImage) : Badge

        object None : Badge
    }
}
