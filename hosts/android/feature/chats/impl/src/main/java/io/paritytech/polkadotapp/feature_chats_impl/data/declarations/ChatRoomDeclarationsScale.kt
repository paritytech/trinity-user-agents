package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.ChatResourceLocal
import kotlinx.serialization.Serializable

// The SCALE values that `chat_room_declarations` keeps. They keep the field order of the Chat Extension v2
// wire types, so they encode to the wire bytes, except that a payment destination carries a length prefix.
// The stored bytes have no version prefix: any change to these types requires a migration.

@Serializable
class ChatRoomConfigLocal(
    val productSource: Boolean,
    val reactionDetails: Boolean,
    val participants: Boolean,
    val messageActions: Boolean,
    val participantActions: Boolean,
    val allowedReactions: List<String>?,
    val placeholder: String?,
    val reportSeen: Boolean,
)

@Serializable
class ChatRoomPreviewLocal(
    val order: ChatRoomOrderLocal,
    val timestamp: Long?,
    val badge: ChatRoomBadgeLocal?,
    val text: ChatRoomPreviewTextLocal?,
)

@Serializable
enum class ChatRoomOrderLocal { TIMESTAMP, PIN_TO_TOP }

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
    class Image(val resource: ChatResourceLocal) : ChatRoomBadgeLocal
}

@Serializable
class ChatRoomPreviewTextLocal(val text: String, val ongoing: Boolean)

@Serializable
class ChatToolbarActionLocal(
    val actionId: String,
    val label: String,
    val icon: ChatResourceLocal,
    val handler: ChatActionHandlerLocal,
)

@Serializable
sealed interface ChatActionHandlerLocal {
    @Serializable
    @EnumIndex(0)
    class Deeplink(val url: String) : ChatActionHandlerLocal

    @Serializable
    @EnumIndex(1)
    object Callback : ChatActionHandlerLocal
}

@Serializable
class ChatSuggestionLocal(val suggestionId: String, val text: String, val sendsText: Boolean)

@Serializable
class ChatCommandLocal(val name: String, val description: String, val argumentHint: String?)

@Serializable
sealed interface ChatRoomFooterLocal {
    @Serializable
    @EnumIndex(0)
    class Input(val config: ChatInputConfigLocal) : ChatRoomFooterLocal

    @Serializable
    @EnumIndex(1)
    object Empty : ChatRoomFooterLocal

    @Serializable
    @EnumIndex(2)
    object Custom : ChatRoomFooterLocal
}

@Serializable
class ChatInputConfigLocal(
    val placeholder: String?,
    val photo: Boolean,
    val video: Boolean,
    val file: Boolean,
    val payment: ChatInputPaymentLocal?,
)

@Serializable
sealed interface ChatInputPaymentLocal {
    @Serializable
    @EnumIndex(0)
    class Enabled(val destination: ByteArray) : ChatInputPaymentLocal
}
