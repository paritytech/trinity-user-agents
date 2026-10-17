package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

// The values of the Chat Extension v2 additions. Each feature is opt-in for each room: a room that declares
// nothing shows what a room of Chat Extension v2 shows. The host applies a feature only when it lists the
// name of the feature in its capabilities.

/** The optional features that a product declares for one of its rooms (`ChatRoomFeatures`). */
data class ChatRoomFeaturesModel(
    /** A user attachment above the inline bound becomes a preimage that the product pays for. */
    val preimageAttachments: Boolean,
    /** The product answers `search_query`. It applies only to a room whose messages the product keeps. */
    val search: Boolean,
    /** Null shows no call button. */
    val calls: ChatRoomCallsModel?,
    /** Not null when the room is the thread of a message of another room of the same product. */
    val threadOf: ChatThreadParentModel?,
    /** The host offers "Read aloud" on text messages. */
    val readAloud: Boolean,
    /** A user file above the inline bound goes encrypted, as Bulletin parts that the product pays for. */
    val sealedAttachments: Boolean,
) {
    companion object {
        val HOST_DEFAULT = ChatRoomFeaturesModel(
            preimageAttachments = false,
            search = false,
            calls = null,
            threadOf = null,
            readAloud = false,
            sealedAttachments = false,
        )
    }
}

/** The call buttons in the header of a room. */
data class ChatRoomCallsModel(val audio: Boolean, val video: Boolean)

/** The message of another room that a thread room answers. Both ids are the ids of the product. */
data class ChatThreadParentModel(val roomId: String, val messageId: String)

/** What the host draws for one message besides its body (`ChatMessageMeta`). */
data class ChatMessageMetaModel(
    val thread: ChatThreadSummaryModel?,
    val branch: ChatBranchPositionModel?,
    val reasoning: ChatReasoningModel?,
) {
    /** A meta without a part removes the meta of the message. */
    val isEmpty: Boolean
        get() = thread == null && branch == null && reasoning == null
}

/** The summary under a message that has a thread. */
data class ChatThreadSummaryModel(
    /** The thread room; its `threadOf` names this message. */
    val roomId: String,
    val replyCount: Long,
    /** Milliseconds since the Unix epoch. */
    val lastReplyAt: Long?,
    /** Up to three repliers. */
    val participantIds: List<String>,
)

/** Which of several alternatives of a message shows. [index] starts at 1. */
data class ChatBranchPositionModel(val index: Int, val count: Int)

/** The message is the reasoning of the product. [durationMs] is how long the product thought. */
data class ChatReasoningModel(val durationMs: Long?)

/** The call of a room (`ChatRoomCall`). The host keeps it in memory only. */
sealed interface ChatRoomCallModel {
    val callId: String
    val video: Boolean

    /** An incoming call: the room rings. [caller] is a participant id. */
    data class Ringing(override val callId: String, override val video: Boolean, val caller: String?) : ChatRoomCallModel

    /**
     * A call in progress since [startedAt] (milliseconds since the Unix epoch). With [mediaSession], the host
     * shows that session of the Media service. Without it, the host shows only the call bar.
     */
    data class Active(
        override val callId: String,
        override val video: Boolean,
        val startedAt: Long,
        val mediaSession: DataByteArray?,
    ) : ChatRoomCallModel
}

/** What the user set for a product room. The product cannot set it. */
data class ChatRoomUserStateModel(
    /** Milliseconds since the Unix epoch when the mute ends. [MUTED_WITHOUT_END] has no end. Null: not muted. */
    val mutedUntil: Long?,
    val archived: Boolean,
) {
    fun isMutedAt(nowMs: Long): Boolean = mutedUntil != null && mutedUntil > nowMs

    companion object {
        const val MUTED_WITHOUT_END = Long.MAX_VALUE

        val NONE = ChatRoomUserStateModel(mutedUntil = null, archived = false)
    }
}

/** What the user does with the features of the additions in a room. The host sends it to the product. */
sealed interface ChatRoomEventModel {
    /** The user muted, unmuted, archived or unarchived the room. */
    data class UserStateChanged(val state: ChatRoomUserStateModel) : ChatRoomEventModel

    /** The user pressed a call button of the header. */
    data class CallRequested(val video: Boolean) : ChatRoomEventModel

    /** The user answered the ringing call. */
    data class CallAnswered(val callId: String, val accept: Boolean) : ChatRoomEventModel

    /** The user opened the call bar of a call without a media session. */
    data class CallOpened(val callId: String) : ChatRoomEventModel

    /** The user pressed "End" on the call bar. */
    data class CallEnded(val callId: String) : ChatRoomEventModel

    /** The user selected another alternative of a message. [messageId] is the id of the product. */
    data class BranchSelected(val messageId: String, val index: Int) : ChatRoomEventModel
}

/**
 * A bot persona of a product: the host draws the participant `bot:<botId>` with [name] and [icon].
 * [icon] is the address that the product gave (`https` or `data:`).
 */
data class ChatBotPersonaModel(val botId: String, val name: String, val icon: String)
