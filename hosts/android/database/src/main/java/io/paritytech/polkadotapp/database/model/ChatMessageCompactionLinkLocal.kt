package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

@Entity(
    tableName = "chat_message_compaction_links",
    indices = [
        Index(value = ["commitId"])
    ]
)
class ChatMessageCompactionLinkLocal(
    @PrimaryKey val originalId: String,
    val commitId: String
)
