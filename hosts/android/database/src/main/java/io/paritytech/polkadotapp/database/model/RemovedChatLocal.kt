package io.paritytech.polkadotapp.database.model

import androidx.room.Entity
import androidx.room.PrimaryKey

/** Tombstone for chats removed locally — lets inter-own-device sync replicate the deletion. */
@Entity(tableName = "removed_chats")
class RemovedChatLocal(
    @PrimaryKey val accountId: ByteArray,
    val removedAt: Long,
)
