package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "message_revisions",
    foreignKeys = [
        ForeignKey(
            entity = ChatMessageLocal::class,
            parentColumns = ["id"],
            childColumns = ["messageId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["chatId"])
    ],
    primaryKeys = ["messageId", "timestamp"]
)
class MessageRevisionLocal(
    val messageId: String,
    val type: ChatMessageLocal.Type,
    val content: ByteArray,
    val chatId: ByteArray,
    val timestamp: Long
)
