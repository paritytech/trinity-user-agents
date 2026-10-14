package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index

@Entity(
    tableName = "chat_message_processing",
    indices = [
        Index(value = ["chatId"])
    ],
    foreignKeys = [
        ForeignKey(
            entity = ChatMessageLocal::class,
            parentColumns = ["id"],
            childColumns = ["messageId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    primaryKeys = ["chatId", "messageId", "middlewareId"]
)
class ChatMessageProcessingLocal(
    val chatId: ByteArray,
    val messageId: String,
    val middlewareId: String
)
