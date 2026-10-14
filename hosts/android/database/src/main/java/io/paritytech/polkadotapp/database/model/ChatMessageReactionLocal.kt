package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "chat_message_reactions",
    foreignKeys = [
        ForeignKey(
            entity = ChatMessageLocal::class,
            parentColumns = ["id"],
            childColumns = ["messageId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [
        Index(value = ["chatId"]),
    ],
    primaryKeys = ["messageId", "emoji", "origintype"]
)
class ChatMessageReactionLocal(
    val messageId: String,
    val emoji: String,
    @Embedded(prefix = "origin")
    val origin: ChatMessageLocal.Origin,
    val chatId: ByteArray,
    val timestamp: Long
)
