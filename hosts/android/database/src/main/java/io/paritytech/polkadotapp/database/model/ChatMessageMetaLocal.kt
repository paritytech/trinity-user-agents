package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index

/**
 * The thread summary, the branch position and the reasoning flag that a product set for one message of its
 * room (Chat Extension v2 additions). [messageId] is the id under which the product knows the message, so the
 * row also serves a room whose messages the product keeps. [timestamp] orders the rows when the host keeps
 * only the newest ones of a room. [meta] is a version byte, then the SCALE value.
 */
@Entity(
    tableName = "chat_message_meta",
    primaryKeys = ["chatId", "messageId"],
    indices = [Index(value = ["chatId", "timestamp"])],
)
class ChatMessageMetaLocal(
    val chatId: ByteArray,
    val messageId: String,
    val timestamp: Long,
    val meta: ByteArray,
)
