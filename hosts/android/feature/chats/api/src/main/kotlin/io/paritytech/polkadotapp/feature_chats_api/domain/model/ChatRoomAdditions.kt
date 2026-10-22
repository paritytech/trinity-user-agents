package io.paritytech.polkadotapp.feature_chats_api.domain.model

// The values of the Chat Extension v2 additions. Each feature is opt-in for each room: a room that declares
// nothing shows what a room of Chat Extension v2 shows. The host applies a feature only when it lists the
// name of the feature in its capabilities.

/** The optional features that a product declares for one of its rooms (`ChatRoomFeatures`). */
data class ChatRoomFeaturesModel(
    /** Null shows no call button. */
    val calls: ChatRoomCallsModel?,
    /** A user photo, video or file goes to a HOP node of the person chat, as the native chat sends one. */
    val hopAttachments: Boolean,
) {
    companion object {
        val HOST_DEFAULT = ChatRoomFeaturesModel(calls = null, hopAttachments = false)
    }
}

/** The call buttons in the header of a room. */
data class ChatRoomCallsModel(val audio: Boolean, val video: Boolean)

/** The call of a room (`ChatRoomCall`). The host keeps it in memory only. */
sealed interface ChatRoomCallModel {
    val callId: String
    val video: Boolean

    /** An incoming call: the host rings. [caller] is a participant id. */
    data class Ringing(override val callId: String, override val video: Boolean, val caller: String?) : ChatRoomCallModel

    /** A call in progress since [startedAt] (milliseconds since the Unix epoch). */
    data class Active(
        override val callId: String,
        override val video: Boolean,
        val startedAt: Long,
    ) : ChatRoomCallModel
}

/** What the user does with the features of the additions in a room. The host sends it to the product. */
sealed interface ChatRoomEventModel {
    /**
     * The user pressed a call button of the header, or a call message to call again. [callId] is the id that the host
     * chose: the product uses it as the call id and as the id of the offer message.
     */
    data class CallRequested(val video: Boolean, val callId: String? = null) : ChatRoomEventModel

    /** The user answered the ringing call. */
    data class CallAnswered(val callId: String, val accept: Boolean) : ChatRoomEventModel

    /** The user ended the call. */
    data class CallEnded(val callId: String) : ChatRoomEventModel
}
