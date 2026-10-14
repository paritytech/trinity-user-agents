package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded
import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_message_pending_expansions")
class ChatMessagePendingExpansionLocal(
    @PrimaryKey val commitId: String,
    @Embedded
    val retryState: TransferRetryStateLocal
)
