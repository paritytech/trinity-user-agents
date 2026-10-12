package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo
import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

@Entity(
    tableName = "chat_messages",
    indices = [
        Index(value = ["chatId"]),
        Index(value = ["chatId", "timestamp"]),
        Index(value = ["sortOrder"]),
        Index(value = ["chatId", "sortOrder", "timestamp"])
    ]
)
class ChatMessageLocal(
    @PrimaryKey val id: String,
    // TODO v1: primary key to ChatRoom. Needs non-trivial migration
    val chatId: ByteArray,
    val timestamp: Long,
    @ColumnInfo(defaultValue = "0") val updatedAt: Long,
    @ColumnInfo(defaultValue = "$UNORDERED") val sortOrder: Long,
    @Embedded(prefix = "origin")
    val origin: Origin,
    val status: Status,
    val type: Type,
    val searchableContent: String,
    val content: ByteArray,
    val replyToMessageId: String? = null,
    val isInternal: Boolean
) {
    companion object {
        const val UNORDERED = 0L
    }

    class Origin(
        val type: OriginType,
        val key: ByteArray?
    )

    enum class OriginType {
        USER, CONTACT, MIDDLEWARE
    }

    enum class Status {
        PROCESSING, NEW, IS_SENT, IS_READ, DELIVERY_FAILED
    }

    enum class Type {
        TEXT,
        TOKEN,
        PAYMENT,
        CONTACT_ADDED,
        RICH_TEXT,
        REACTED,
        REACTION_REMOVED,
        UNSUPPORTED,
        LEFT_CHAT,
        EDITED,
        DATA_CHANNEL_OFFER,
        DATA_CHANNEL_ANSWER,
        DATA_CHANNEL_CANDIDATE,
        DATA_CHANNEL_CLOSED,
        CUSTOM,
        CHAT_ACCEPTED,
        CHAT_REQUEST,
        DEVICE_ADDED,
        DEVICE_REMOVED,
        COMPACTION_COMMIT,
        COMPACTION_UNAVAILABLE
    }
}
