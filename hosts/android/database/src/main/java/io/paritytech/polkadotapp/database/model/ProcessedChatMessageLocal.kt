package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.PrimaryKey

@Entity(
    tableName = "processed_chat_messages",
    foreignKeys = [
        ForeignKey(
            entity = ChatMessageLocal::class,
            parentColumns = ["id"],
            childColumns = ["messageId"],
            onDelete = ForeignKey.CASCADE
        )
    ]
)
class ProcessedChatMessageLocal(
    @PrimaryKey val messageId: String
)
