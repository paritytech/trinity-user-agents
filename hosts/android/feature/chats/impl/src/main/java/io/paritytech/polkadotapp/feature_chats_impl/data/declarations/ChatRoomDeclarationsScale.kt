package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatMessageContentLocal
import kotlinx.serialization.Serializable

// The SCALE values that `chat_room_declarations` keeps. They keep the field order of the Chat Extension v2
// wire types, except that a payment destination carries a length prefix and a preview message keeps the stored
// content of a message. The stored bytes have no version prefix: a value that does not decode reads as "not
// declared", and the product declares it again.

/**
 * The configuration keeps its first stored layout, so the rooms keep their configuration. Chat Extension v2 dropped
 * [participants], [messageActions], [allowedReactions] and [placeholder]: they are written empty and not read.
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
)

@Serializable
sealed interface ChatRoomBadgeLocal {
    @Serializable
    @EnumIndex(0)
    object Hidden : ChatRoomBadgeLocal

    @Serializable
    @EnumIndex(1)
    class UnreadCount(val count: Int) : ChatRoomBadgeLocal
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

@Serializable
class ChatRoomContactLocal(val account: ByteArray, val username: String?, val state: ChatContactStateLocal)

@Serializable
enum class ChatContactStateLocal { INVITE, OUTGOING, INCOMING, ESTABLISHED, BLOCKED, PEER_LEFT }
