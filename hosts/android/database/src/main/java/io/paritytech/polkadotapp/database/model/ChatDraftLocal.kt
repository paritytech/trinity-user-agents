package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_drafts")
class ChatDraftLocal(
    @PrimaryKey val chatId: ByteArray,
    val text: String,
    val relation: ByteArray,
)
