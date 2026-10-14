package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "chat_request_sync_state")
class ChatRequestSyncStateLocal(
    @PrimaryKey val metaAccountId: Long,
    val lastSyncedDay: Long?,
)
