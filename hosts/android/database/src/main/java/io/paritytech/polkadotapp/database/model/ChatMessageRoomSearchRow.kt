package io.paritytech.polkadotapp.database.model

/** A stored message that the in-room search reads: its text and its author. */
class ChatMessageRoomSearchRow(
    val id: String,
    val timestamp: Long,
    val searchableContent: String,
    val originType: ChatMessageLocal.OriginType,
    /** The participant of a product room that wrote the message. Null elsewhere. */
    val originParticipantId: String?,
)
