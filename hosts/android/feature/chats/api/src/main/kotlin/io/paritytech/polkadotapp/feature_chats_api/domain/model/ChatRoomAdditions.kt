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
    /** The host offers "Read aloud" on text messages. */
    val readAloud: Boolean,
    /** A user file above the inline bound goes encrypted, as Bulletin parts that the product pays for. */
    val sealedAttachments: Boolean,
    /** A user photo, video or file goes to a HOP node of the person chat, as the native chat sends one. */
    val hopAttachments: Boolean,
) {
    companion object {
        val HOST_DEFAULT = ChatRoomFeaturesModel(
            preimageAttachments = false,
            search = false,
            calls = null,
            readAloud = false,
            sealedAttachments = false,
            hopAttachments = false,
        )
    }
}

/** The call buttons in the header of a room. */
data class ChatRoomCallsModel(val audio: Boolean, val video: Boolean)

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
}

/**
 * A bot persona of a product: the host draws the participant `bot:<botId>` with [name] and [icon].
 * [icon] is the address that the product gave (`https` or `data:`).
 */
data class ChatBotPersonaModel(val botId: String, val name: String, val icon: String)
