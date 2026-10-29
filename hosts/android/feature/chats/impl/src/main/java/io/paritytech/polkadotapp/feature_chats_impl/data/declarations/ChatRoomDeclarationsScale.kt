package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import kotlinx.serialization.Serializable

// The SCALE values that `chat_room_declarations` keeps. They keep the field order of the Chat Extension v2
// wire types, except that a preview message keeps the stored content of a message. The stored bytes have no
// version prefix: a value that does not decode reads as "not declared", and the product declares it again.

/**
 * The configuration keeps its first stored layout, so the rooms keep their configuration. Chat Extension v2 does not
 * have [participants], [messageActions], [allowedReactions] and [placeholder]: they are written empty and not read.
 */
@Serializable
class ChatRoomConfigLocal(
    val productSource: Boolean,
    val reactionDetails: Boolean,
    val participants: Boolean,
    val messageActions: Boolean,
    val allowedReactions: List<String>?,
    val placeholder: String?,
    val reportSeen: Boolean,
)

@Serializable
class ChatRoomPreviewLocal(
    val timestamp: Long?,
    val badge: ChatRoomBadgeLocal?,
    val lastMessage: PreviewMessageLocal?,
    val lastReaction: PreviewReactionLocal?,
    val unseenReactions: Int,
    val pinned: Boolean,
    val activity: ChatRoomActivityLocal?,
)

/** The first stored layout of a preview. It has no [ChatRoomPreviewLocal.pinned] and no [ChatRoomPreviewLocal.activity]. */
@Serializable
class ChatRoomPreviewFirstLayoutLocal(
    val timestamp: Long?,
    val badge: ChatRoomBadgeLocal?,
    val lastMessage: PreviewMessageLocal?,
    val lastReaction: PreviewReactionLocal?,
    val unseenReactions: Int,
)

@Serializable
class ChatRoomActivityLocal(val text: String, val ongoing: Boolean)

@Serializable
sealed interface ChatRoomBadgeLocal {
    @Serializable
    @EnumIndex(0)
    object Hidden : ChatRoomBadgeLocal

    @Serializable
    @EnumIndex(1)
    class UnreadCount(val count: Int) : ChatRoomBadgeLocal

    @Serializable
    @EnumIndex(2)
    class Image(val icon: String) : ChatRoomBadgeLocal
}

@Serializable
class PreviewMessageLocal(
    val messageId: String,
    val participantId: String,
    val timestamp: Long,
    val content: ChatMessageContentLocal,
    val status: PreviewMessageStatusLocal,
    val replyTo: String?,
    val editedAt: Long?,
    val request: Boolean,
)

@Serializable
enum class PreviewMessageStatusLocal { PREPARING, SENT, DELIVERED, READ, FAILED }

@Serializable
class PreviewReactionLocal(val emoji: String, val byUser: Boolean, val message: PreviewMessageLocal?, val removed: Boolean)
