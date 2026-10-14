package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(
    tableName = "chat_search_recents"
)
class ChatSearchRecentLocal(
    @PrimaryKey
    val chatId: ByteArray,
    val timestamp: Long,
)
